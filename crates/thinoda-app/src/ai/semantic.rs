use super::*;
use thinoda_ai::AiEmbeddingJob;

#[cfg(test)]
#[path = "semantic_tests.rs"]
mod tests;
const MAX_TOPICS: usize = 500;
const MAX_TEXT_BYTES: usize = 65_536;

struct Source {
    key: String,
    sheet: usize,
    root: TopicId,
    revision: u64,
    config: AiConfig,
    model: String,
    viewport: egui::ViewportId,
    topics: Vec<(TopicId, String)>,
}

impl Source {
    fn valid(&self, app: &ThinodaApp) -> bool {
        app.settings.ai.enabled
            && self.config == embedding_config(&app.settings.ai)
            && self.model == app.settings.ai.embedding_model.trim()
            && app.docs.iter().any(|doc| {
                doc.key == self.key
                    && doc.doc.active_index() == self.sheet
                    && doc.doc.sheet().root == self.root
                    && doc.doc.state_id() == self.revision
                    && doc.inspector_edit.is_none()
                    && doc.view.editing_topic().is_none()
            })
    }
}

struct Cache {
    source: Source,
    vectors: Vec<Vec<f32>>,
}

enum Operation {
    Index { vectors: Vec<Vec<f32>>, end: usize },
    Query(String),
}

struct Pending {
    job: AiEmbeddingJob,
    source: Source,
    operation: Operation,
}

pub struct SemanticState {
    query: String,
    composing: bool,
    cache: Option<Cache>,
    pending: Option<Pending>,
    results: Vec<(TopicId, f64)>,
    clusters: Vec<Vec<usize>>,
    clustering: Option<ClusterJob>,
    cluster_count: usize,
    error: Option<String>,
}

impl Default for SemanticState {
    fn default() -> Self {
        Self {
            query: String::new(),
            composing: false,
            cache: None,
            pending: None,
            results: Vec::new(),
            clusters: Vec::new(),
            clustering: None,
            cluster_count: 3,
            error: None,
        }
    }
}

fn normalize(vectors: Vec<Vec<f32>>) -> Result<Vec<Vec<f32>>, String> {
    vectors
        .into_iter()
        .map(|vector| {
            let norm = vector
                .iter()
                .map(|&v| f64::from(v).powi(2))
                .sum::<f64>()
                .sqrt();
            if norm == 0.0 || !norm.is_finite() {
                return Err(t("The embedding model returned an unusable vector.").into());
            }
            Ok(vector
                .into_iter()
                .map(|v| (f64::from(v) / norm) as f32)
                .collect())
        })
        .collect()
}

fn cosine(a: &[f32], b: &[f32]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(&a, &b)| f64::from(a) * f64::from(b))
        .sum()
}

/// Farthest-first spherical k-means, processed in small chunks on native and web.
struct ClusterJob {
    count: usize,
    centers: Vec<Vec<f32>>,
    cursor: usize,
    best: (usize, f64),
    seeding: bool,
    iteration: usize,
    assignments: Vec<usize>,
    sums: Vec<Vec<f64>>,
}

impl ClusterJob {
    fn new(vectors: &[Vec<f32>], count: usize) -> Self {
        Self {
            count: count.clamp(1, vectors.len()),
            centers: vec![vectors[0].clone()],
            cursor: 0,
            best: (0, f64::INFINITY),
            seeding: count > 1,
            iteration: 0,
            assignments: vec![0; vectors.len()],
            sums: vec![vec![0.0; vectors[0].len()]],
        }
    }

    fn step(&mut self, vectors: &[Vec<f32>]) -> Option<Vec<Vec<usize>>> {
        let end = (self.cursor + 16).min(vectors.len());
        for (index, vector) in vectors.iter().enumerate().take(end).skip(self.cursor) {
            if self.seeding {
                let similarity = self
                    .centers
                    .iter()
                    .map(|center| cosine(vector, center))
                    .fold(f64::NEG_INFINITY, f64::max);
                if similarity < self.best.1 {
                    self.best = (index, similarity);
                }
            } else {
                let group = self
                    .centers
                    .iter()
                    .enumerate()
                    .max_by(|(a, va), (b, vb)| {
                        cosine(vector, va)
                            .total_cmp(&cosine(vector, vb))
                            .then(b.cmp(a))
                    })
                    .unwrap()
                    .0;
                self.assignments[index] = group;
                for (sum, &value) in self.sums[group].iter_mut().zip(vector) {
                    *sum += f64::from(value);
                }
            }
        }
        self.cursor = end;
        if end < vectors.len() {
            return None;
        }
        self.cursor = 0;
        if self.seeding {
            if self.best.1 < 0.999_999 {
                self.centers.push(vectors[self.best.0].clone());
            }
            if self.centers.len() >= self.count || self.best.1 >= 0.999_999 {
                self.seeding = false;
                self.sums = vec![vec![0.0; vectors[0].len()]; self.centers.len()];
            }
            self.best = (0, f64::INFINITY);
            return None;
        }
        self.iteration += 1;
        if self.iteration == 8 {
            let mut groups = vec![Vec::new(); self.centers.len()];
            for (index, &group) in self.assignments.iter().enumerate() {
                groups[group].push(index);
            }
            groups.retain(|group| !group.is_empty());
            return Some(groups);
        }
        for (center, sum) in self.centers.iter_mut().zip(&mut self.sums) {
            let norm = sum.iter().map(|value| value * value).sum::<f64>().sqrt();
            if norm > 0.0 {
                *center = sum.iter().map(|value| (value / norm) as f32).collect();
            }
            sum.fill(0.0);
        }
        None
    }
}

impl ThinodaApp {
    fn semantic_source(&self) -> Result<Source, String> {
        if !self.settings.ai.enabled {
            return Err(t("AI assistant is disabled in Settings").into());
        }
        let model = self.settings.ai.embedding_model.trim();
        if model.is_empty() {
            return Err(t("Set an explicit embedding model in Settings before indexing.").into());
        }
        let Some(doc) = self.doc() else {
            return Err(t("Not possible here").into());
        };
        if doc.inspector_edit.is_some() || doc.view.editing_topic().is_some() {
            return Err(t("Finish the current edit before sending.").into());
        }
        let topics = &doc.doc.sheet().topics;
        if topics.len() > MAX_TOPICS
            || topics
                .values()
                .map(|topic| topic.title.len())
                .sum::<usize>()
                > MAX_TEXT_BYTES
        {
            return Err(
                t("Semantic indexing is limited to 500 topics and 64 KiB of titles.").into(),
            );
        }
        if topics.values().any(|topic| {
            topic.title.trim().is_empty()
                || topic.title.len() > thinoda_ai::embeddings::MAX_TEXT_BYTES
        }) {
            return Err(
                t("Semantic indexing requires nonempty titles of at most 16 KiB each.").into(),
            );
        }
        Ok(Source {
            key: doc.key.clone(),
            sheet: doc.doc.active_index(),
            root: doc.doc.sheet().root,
            revision: doc.doc.state_id(),
            config: embedding_config(&self.settings.ai),
            model: model.into(),
            viewport: self.current_viewport,
            topics: topics
                .values()
                .map(|topic| (topic.id, topic.title.clone()))
                .collect(),
        })
    }

    fn start_semantic_index(&mut self) -> Result<(), String> {
        if self.ai.semantic.pending.is_some() {
            return Err(t("AI is still working…").into());
        }
        let source = self.semantic_source()?;
        let end = source.topics.len().min(thinoda_ai::embeddings::MAX_INPUTS);
        let inputs = source.topics[..end]
            .iter()
            .map(|(_, title)| title.clone())
            .collect();
        let job = AiEmbeddingJob::spawn(source.config.clone(), source.model.clone(), inputs);
        self.ai.semantic = SemanticState {
            query: self.ai.semantic.query.clone(),
            pending: Some(Pending {
                job,
                source,
                operation: Operation::Index {
                    vectors: Vec::new(),
                    end,
                },
            }),
            ..Default::default()
        };
        Ok(())
    }

    fn start_semantic_query(&mut self) -> Result<(), String> {
        let state = &self.ai.semantic;
        if state.pending.is_some() {
            return Err(t("AI is still working…").into());
        }
        if state.query.trim().is_empty() || state.query.len() > 8192 {
            return Err(t("Enter a prompt of at most 8192 bytes.").into());
        }
        let Some(cache) = &state.cache else {
            return Err(t("Build the semantic index first.").into());
        };
        if !cache.source.valid(self) {
            return Err(t("The document changed. Try again.").into());
        }
        let source = self.semantic_source()?;
        if source.key != cache.source.key || source.sheet != cache.source.sheet {
            return Err(t("Build the semantic index first.").into());
        }
        let text = self.ai.semantic.query.clone();
        let job = AiEmbeddingJob::spawn(
            source.config.clone(),
            source.model.clone(),
            vec![text.clone()],
        );
        self.ai.semantic.pending = Some(Pending {
            job,
            source,
            operation: Operation::Query(text),
        });
        self.ai.semantic.results.clear();
        self.ai.semantic.error = None;
        Ok(())
    }

    fn cancel_semantic(&mut self) {
        self.ai.semantic.clustering = None;
        if let Some(pending) = self.ai.semantic.pending.take() {
            pending.job.cancel();
        }
    }

    pub(super) fn poll_semantic(&mut self, ctx: &Context) {
        if self
            .ai
            .semantic
            .cache
            .as_ref()
            .is_some_and(|cache| !cache.source.valid(self))
        {
            self.ai.semantic.cache = None;
            self.ai.semantic.results.clear();
            self.ai.semantic.clusters.clear();
            self.ai.semantic.clustering = None;
            self.ai.semantic.error =
                Some(t("Semantic source or settings changed. Rebuild the index.").into());
        }
        if let Some(job) = &mut self.ai.semantic.clustering
            && let Some(cache) = &self.ai.semantic.cache
        {
            if let Some(groups) = job.step(&cache.vectors) {
                self.ai.semantic.clusters = groups;
                self.ai.semantic.clustering = None;
            }
            ctx.request_repaint_of(cache.source.viewport);
        }
        let Some(pending) = &self.ai.semantic.pending else {
            return;
        };
        if !pending.source.valid(self) {
            self.cancel_semantic();
            self.ai.semantic.error =
                Some(t("Semantic source or settings changed. Rebuild the index.").into());
            return;
        }
        if let Operation::Query(text) = &pending.operation
            && text != &self.ai.semantic.query
        {
            self.cancel_semantic();
            return;
        }
        let Some(result) = pending.job.poll() else {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return;
        };
        let mut pending = self.ai.semantic.pending.take().unwrap();
        ctx.request_repaint_of(pending.source.viewport);
        let result = result
            .map_err(|error| error_text(&error))
            .and_then(normalize);
        let vectors = match result {
            Ok(vectors) => vectors,
            Err(error) => {
                self.ai.semantic.error = Some(error);
                return;
            }
        };
        match pending.operation {
            Operation::Index {
                vectors: mut accumulated,
                end,
            } => {
                if accumulated.first().is_some_and(|previous| {
                    vectors
                        .first()
                        .is_some_and(|next| previous.len() != next.len())
                }) {
                    self.ai.semantic.error =
                        Some(t("The embedding model changed vector dimensions.").into());
                    return;
                }
                accumulated.extend(vectors);
                if end == pending.source.topics.len() {
                    self.ai.semantic.cache = Some(Cache {
                        source: pending.source,
                        vectors: accumulated,
                    });
                    self.ai.semantic.error = None;
                } else {
                    let next_end =
                        (end + thinoda_ai::embeddings::MAX_INPUTS).min(pending.source.topics.len());
                    let inputs = pending.source.topics[end..next_end]
                        .iter()
                        .map(|(_, text)| text.clone())
                        .collect();
                    pending.job = AiEmbeddingJob::spawn(
                        pending.source.config.clone(),
                        pending.source.model.clone(),
                        inputs,
                    );
                    pending.operation = Operation::Index {
                        vectors: accumulated,
                        end: next_end,
                    };
                    self.ai.semantic.pending = Some(pending);
                }
            }
            Operation::Query(_) => {
                let Some(cache) = &self.ai.semantic.cache else {
                    return;
                };
                if vectors[0].len() != cache.vectors[0].len() {
                    self.ai.semantic.error =
                        Some(t("The embedding model changed vector dimensions.").into());
                    return;
                }
                let mut results: Vec<_> = cache
                    .source
                    .topics
                    .iter()
                    .zip(&cache.vectors)
                    .map(|((id, _), vector)| (*id, cosine(vector, &vectors[0])))
                    .collect();
                results.sort_by(|(ia, a), (ib, b)| b.total_cmp(a).then(ia.cmp(ib)));
                results.truncate(20);
                self.ai.semantic.results = results;
            }
        }
    }

    fn apply_semantic_clusters(&mut self) -> Result<(), String> {
        let state = &self.ai.semantic;
        let Some(cache) = &state.cache else {
            return Err(t("Build the semantic index first.").into());
        };
        if !cache.source.valid(self) || state.clusters.is_empty() {
            return Err(t("The document changed. Try again.").into());
        }
        let outline = thinoda_ai::Outline {
            title: Some(t("Semantic clusters").into()),
            items: state
                .clusters
                .iter()
                .enumerate()
                .map(|(group, indices)| thinoda_ai::OutlineNode {
                    title: format!("{} {}", t("Group"), group + 1),
                    children: indices
                        .iter()
                        .map(|&index| thinoda_ai::OutlineNode::new(&cache.source.topics[index].1))
                        .collect(),
                })
                .collect(),
        };
        let key = cache.source.key.clone();
        let mut book = workbook_from_outline(&outline);
        let doc = self.docs.iter_mut().find(|doc| doc.key == key).unwrap();
        doc.doc.add_sheet(book.sheets.remove(0));
        doc.view.reset();
        self.cancel_semantic();
        self.ai.semantic.cache = None;
        self.ai.semantic.results.clear();
        self.ai.semantic.clusters.clear();
        Ok(())
    }

    pub(super) fn semantic_panel(&mut self, ui: &mut Ui) {
        egui::CollapsingHeader::new(t("Semantic tools")).id_salt("semantic_tools").show(ui, |ui| {
            widgets::caption(ui, t("Build sends current-sheet titles to the embedding model. No background indexing."));
            ui.add(egui::Label::new(format!("{}: {}", t("Server"), self.settings.ai.base_url)).truncate())
                .on_hover_text(&self.settings.ai.base_url);
            ui.label(format!("{}: {}", t("Embedding model"), self.settings.ai.embedding_model));
            let allowed = widgets::touch_activation_allowed(ui.ctx());
            let mut error = None;
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(self.ai.semantic.pending.is_none(), egui::Button::new(t("Build semantic index"))).clicked() && allowed {
                    error = self.start_semantic_index().err();
                }
                if self.ai.semantic.pending.is_some() || self.ai.semantic.clustering.is_some() {
                    ui.spinner();
                    if ui.button(t("Cancel indexing or search")).clicked() && allowed { self.cancel_semantic(); }
                }
            });
            let here = self.ai.semantic.cache.as_ref().is_some_and(|cache| {
                cache.source.valid(self) && self.doc().is_some_and(|doc| doc.key == cache.source.key)
            });
            if here {
                let label = ui.label(t("Semantic query"));
                if ui.add(egui::TextEdit::singleline(&mut self.ai.semantic.query)
                    .id(egui::Id::unique("semantic_query"))
                    .char_limit(8192)).labelled_by(label.id).changed() {
                    self.ai.semantic.results.clear();
                }
                if ui.add_enabled(self.ai.semantic.pending.is_none(), egui::Button::new(t("Search meaning"))).clicked() && allowed {
                    error = self.start_semantic_query().err();
                }
                let mut selected = None;
                let cache = self.ai.semantic.cache.as_ref().unwrap();
                for &(id, score) in &self.ai.semantic.results {
                    let title = cache.source.topics.iter().find(|(topic, _)| *topic == id).map(|(_, title)| title.as_str()).unwrap();
                    if ui.add(egui::Button::new(format!("{score:.2}  {title}")).wrap()).clicked() && allowed { selected = Some(id); }
                }
                let old_count = self.ai.semantic.cluster_count;
                ui.horizontal_wrapped(|ui| {
                    ui.label(t("Cluster count"));
                    ui.add(egui::DragValue::new(&mut self.ai.semantic.cluster_count).range(2..=8))
                        .accessible_name(t("Cluster count"));
                });
                if !allowed { self.ai.semantic.cluster_count = old_count; }
                else if old_count != self.ai.semantic.cluster_count {
                    self.ai.semantic.clusters.clear();
                    self.ai.semantic.clustering = None;
                }
                if ui.add_enabled(self.ai.semantic.clustering.is_none(), egui::Button::new(t("Preview clusters"))).clicked() && allowed {
                    self.ai.semantic.clusters.clear();
                    self.ai.semantic.clustering = Some(ClusterJob::new(&cache.vectors, self.ai.semantic.cluster_count));
                }
                for (index, group) in self.ai.semantic.clusters.iter().enumerate() {
                    ui.strong(format!("{} {} ({})", t("Group"), index + 1, group.len()));
                    for &topic in group { ui.label(&cache.source.topics[topic].1); }
                }
                if !self.ai.semantic.clusters.is_empty() {
                    widgets::caption(ui, t("Apply creates a new sheet. The source stays unchanged."));
                    if ui.button(t("Apply clusters to new sheet")).clicked() && allowed {
                        error = self.apply_semantic_clusters().err();
                    }
                }
                if let Some(id) = selected
                    && let Some(doc) = self.doc_mut()
                { doc.view.select_only(id); doc.view.reveal(id); }
            }
            if let Some(error) = error { self.ai.semantic.error = Some(error); }
            if let Some(error) = &self.ai.semantic.error { ui.colored_label(ui.visuals().error_fg_color, error); }
        });
    }

    pub(super) fn return_semantic_viewport(&mut self, viewport: egui::ViewportId) {
        if let Some(pending) = &mut self.ai.semantic.pending
            && pending.source.viewport == viewport
        {
            pending.source.viewport = egui::ViewportId::ROOT;
        }
        if let Some(cache) = &mut self.ai.semantic.cache
            && cache.source.viewport == viewport
        {
            cache.source.viewport = egui::ViewportId::ROOT;
        }
    }

    pub(super) fn protect_semantic_ime(&mut self, ctx: &Context) {
        let id = egui::Id::unique("semantic_query");
        if self.ai.semantic.composing || ctx.memory(|memory| memory.has_focus(id)) {
            thinoda_canvas::protect_ime_candidate_keys(ctx, id, &mut self.ai.semantic.composing);
        }
    }
}
