//! AI tasks offered in the app (modeled after XMind AI: brainstorm, to-do breakdown,
//! explain, summarize branch and generate a map from a prompt) and their prompts.

use crate::Message;
use crate::outline::{OutlineNode, to_markdown};

/// Context of the topic an AI command runs on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TopicContext {
    /// Titles from the central topic down to (and including) the target topic.
    pub path: Vec<String>,
    /// Existing subtopics of the target topic (so suggestions don't repeat them).
    pub children: Vec<String>,
    /// The target topic's subtree (for summaries).
    pub subtree: Vec<OutlineNode>,
}

impl TopicContext {
    pub fn topic(&self) -> &str {
        self.path.last().map(String::as_str).unwrap_or("")
    }

    fn path_line(&self) -> String {
        self.path.join(" > ")
    }
}

/// What the assistant should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiTask {
    /// New subtopic ideas for a topic.
    Brainstorm(TopicContext),
    /// Actionable to-do steps for a topic.
    Breakdown(TopicContext),
    /// A short explanation of a topic, stored as its note.
    Explain(TopicContext),
    /// A summary of the topic's branch, stored as its note.
    Summarize(TopicContext),
    /// A whole new map from a free-text prompt.
    GenerateMap(String),
}

/// How the app should apply the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputKind {
    /// Add the outline items as subtopics.
    Subtopics,
    /// Add the outline items as unchecked to-do subtopics.
    Todos,
    /// Store plain text as a note.
    Note,
    /// Create a new document from the outline.
    Map,
}

const LIST_RULES: &str = "Reply ONLY with a Markdown bullet list (\"- item\"). \
Keep every item short (at most 8 words). No introduction, no explanations, no bold text, no numbering. \
Use the same language as the topic text.";

const TEXT_RULES: &str = "Reply with plain text only (no Markdown, no lists), 2 to 4 sentences. \
Use the same language as the topic text.";

const SYSTEM: &str = "You are the mind-mapping assistant of yMind, a mind map editor.";

impl AiTask {
    pub fn output_kind(&self) -> OutputKind {
        match self {
            AiTask::Brainstorm(_) => OutputKind::Subtopics,
            AiTask::Breakdown(_) => OutputKind::Todos,
            AiTask::Explain(_) | AiTask::Summarize(_) => OutputKind::Note,
            AiTask::GenerateMap(_) => OutputKind::Map,
        }
    }

    /// Chat messages for this task.
    pub fn messages(&self) -> Vec<Message> {
        let (rules, user) = match self {
            AiTask::Brainstorm(c) => {
                let mut u = format!("Mind map path: {}\n", c.path_line());
                if !c.children.is_empty() {
                    u.push_str(&format!(
                        "Existing subtopics (do not repeat): {}\n",
                        c.children.join("; ")
                    ));
                }
                u.push_str(&format!(
                    "Suggest 5 to 8 new, distinct subtopics for \"{}\".",
                    c.topic()
                ));
                (LIST_RULES, u)
            }
            AiTask::Breakdown(c) => (
                LIST_RULES,
                format!(
                    "Mind map path: {}\nBreak \"{}\" down into 4 to 8 concrete, actionable steps in a sensible order. \
                     Start every step with a verb.",
                    c.path_line(),
                    c.topic()
                ),
            ),
            AiTask::Explain(c) => (
                TEXT_RULES,
                format!(
                    "Mind map path: {}\nExplain \"{}\" briefly and clearly.",
                    c.path_line(),
                    c.topic()
                ),
            ),
            AiTask::Summarize(c) => (
                TEXT_RULES,
                format!(
                    "Summarize this mind map branch about \"{}\":\n{}",
                    c.topic(),
                    to_markdown(&c.subtree).trim_end()
                ),
            ),
            AiTask::GenerateMap(prompt) => (
                "Reply ONLY with Markdown: the first line is \"# \" followed by a short central topic, \
                 then a nested bullet list (\"- item\", sub-items indented by two spaces) with 4 to 7 main topics, \
                 each having 2 to 5 subtopics. Keep items short (at most 8 words). No introduction, no explanations. \
                 Use the same language as the request.",
                format!("Create a mind map for: {}", prompt.trim()),
            ),
        };
        vec![
            Message::system(format!("{SYSTEM} {rules}")),
            Message::user(user),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> TopicContext {
        TopicContext {
            path: vec!["Vacation".into(), "Packing".into()],
            children: vec!["Clothes".into()],
            subtree: vec![OutlineNode::new("Clothes")],
        }
    }

    #[test]
    fn brainstorm_prompt_mentions_path_and_existing() {
        let m = AiTask::Brainstorm(ctx()).messages();
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].role, "system");
        assert!(m[0].content.contains("bullet list"));
        assert!(m[1].content.contains("Vacation > Packing"));
        assert!(m[1].content.contains("Clothes"));
        assert!(m[1].content.contains("\"Packing\""));
    }

    #[test]
    fn summarize_includes_subtree_and_kinds() {
        let t = AiTask::Summarize(ctx());
        assert!(t.messages()[1].content.contains("- Clothes"));
        assert_eq!(t.output_kind(), OutputKind::Note);
        assert_eq!(AiTask::Breakdown(ctx()).output_kind(), OutputKind::Todos);
        assert_eq!(
            AiTask::GenerateMap("x".into()).output_kind(),
            OutputKind::Map
        );
    }
}
