/* GTK's print backends are dynamically loaded modules. Load the host GTK stack
 * as a unit, instead of letting AppImage's ELF scanner bundle libgtk without its
 * CUPS modules (or mixing bundled GTK with incompatible host modules).
 * The .deb already depends on libgtk-3-0. Missing symbols/runtime are errors.
 * Only the verified main thread calls initialization; the handle lives until exit. */
#include <dlfcn.h>

#define PRINT_GTK_SYMBOLS(X) \
    X(gtk_init_check) \
    X(gtk_print_operation_new) \
    X(gtk_page_setup_new) \
    X(gtk_paper_size_new) \
    X(gtk_page_setup_set_paper_size) \
    X(gtk_paper_size_free) \
    X(gtk_page_setup_set_orientation) \
    X(gtk_page_setup_set_top_margin) \
    X(gtk_page_setup_set_bottom_margin) \
    X(gtk_page_setup_set_left_margin) \
    X(gtk_page_setup_set_right_margin) \
    X(gtk_page_setup_get_top_margin) \
    X(gtk_page_setup_get_bottom_margin) \
    X(gtk_page_setup_get_left_margin) \
    X(gtk_page_setup_get_right_margin) \
    X(gtk_print_operation_set_default_page_setup) \
    X(gtk_print_operation_set_job_name) \
    X(gtk_print_operation_set_unit) \
    X(gtk_print_operation_set_use_full_page) \
    X(gtk_print_operation_set_embed_page_setup) \
    X(gtk_print_operation_set_allow_async) \
    X(gtk_print_operation_set_show_progress) \
    X(gtk_print_operation_set_n_pages) \
    X(gtk_print_operation_run) \
    X(gtk_print_operation_get_error) \
    X(gtk_print_operation_cancel) \
    X(gtk_print_context_get_cairo_context) \
    X(gtk_print_context_get_page_setup) \
    X(gtk_print_context_get_width) \
    X(gtk_print_context_get_height) \
    X(gtk_print_context_get_hard_margins) \
    X(g_signal_connect_data) \
    X(g_object_unref) \
    X(g_error_free) \
    X(cairo_image_surface_create_for_data) \
    X(cairo_save) \
    X(cairo_translate) \
    X(cairo_scale) \
    X(cairo_set_source_surface) \
    X(cairo_pattern_set_filter) \
    X(cairo_get_source) \
    X(cairo_paint) \
    X(cairo_surface_status) \
    X(cairo_status) \
    X(cairo_restore) \
    X(cairo_surface_destroy) \
    X(cairo_status_to_string)

#define DECLARE_SYMBOL(name) static __typeof__(&name) p_##name;
PRINT_GTK_SYMBOLS(DECLARE_SYMBOL)
#undef DECLARE_SYMBOL

static int load_gtk(char *error, size_t error_len) {
    static void *library = NULL;
    if (library) return 1;
    void *handle = dlopen("libgtk-3.so.0", RTLD_NOW | RTLD_LOCAL);
    if (!handle) {
        snprintf(error, error_len, "Cannot load system GTK 3 printing: %s", dlerror());
        return 0;
    }
#define LOAD_SYMBOL(name) \
    p_##name = (__typeof__(p_##name))dlsym(handle, #name); \
    if (!p_##name) { \
        snprintf(error, error_len, "System GTK 3 printing is missing symbol %s", #name); \
        dlclose(handle); \
        return 0; \
    }
    PRINT_GTK_SYMBOLS(LOAD_SYMBOL)
#undef LOAD_SYMBOL
    library = handle;
    return 1;
}

#define gtk_init_check p_gtk_init_check
#define gtk_print_operation_new p_gtk_print_operation_new
#define gtk_page_setup_new p_gtk_page_setup_new
#define gtk_paper_size_new p_gtk_paper_size_new
#define gtk_page_setup_set_paper_size p_gtk_page_setup_set_paper_size
#define gtk_paper_size_free p_gtk_paper_size_free
#define gtk_page_setup_set_orientation p_gtk_page_setup_set_orientation
#define gtk_page_setup_set_top_margin p_gtk_page_setup_set_top_margin
#define gtk_page_setup_set_bottom_margin p_gtk_page_setup_set_bottom_margin
#define gtk_page_setup_set_left_margin p_gtk_page_setup_set_left_margin
#define gtk_page_setup_set_right_margin p_gtk_page_setup_set_right_margin
#define gtk_page_setup_get_top_margin p_gtk_page_setup_get_top_margin
#define gtk_page_setup_get_bottom_margin p_gtk_page_setup_get_bottom_margin
#define gtk_page_setup_get_left_margin p_gtk_page_setup_get_left_margin
#define gtk_page_setup_get_right_margin p_gtk_page_setup_get_right_margin
#define gtk_print_operation_set_default_page_setup p_gtk_print_operation_set_default_page_setup
#define gtk_print_operation_set_job_name p_gtk_print_operation_set_job_name
#define gtk_print_operation_set_unit p_gtk_print_operation_set_unit
#define gtk_print_operation_set_use_full_page p_gtk_print_operation_set_use_full_page
#define gtk_print_operation_set_embed_page_setup p_gtk_print_operation_set_embed_page_setup
#define gtk_print_operation_set_allow_async p_gtk_print_operation_set_allow_async
#define gtk_print_operation_set_show_progress p_gtk_print_operation_set_show_progress
#define gtk_print_operation_set_n_pages p_gtk_print_operation_set_n_pages
#define gtk_print_operation_run p_gtk_print_operation_run
#define gtk_print_operation_get_error p_gtk_print_operation_get_error
#define gtk_print_operation_cancel p_gtk_print_operation_cancel
#define gtk_print_context_get_cairo_context p_gtk_print_context_get_cairo_context
#define gtk_print_context_get_page_setup p_gtk_print_context_get_page_setup
#define gtk_print_context_get_width p_gtk_print_context_get_width
#define gtk_print_context_get_height p_gtk_print_context_get_height
#define gtk_print_context_get_hard_margins p_gtk_print_context_get_hard_margins
#define g_signal_connect_data p_g_signal_connect_data
#define g_object_unref p_g_object_unref
#define g_error_free p_g_error_free
#define cairo_image_surface_create_for_data p_cairo_image_surface_create_for_data
#define cairo_save p_cairo_save
#define cairo_translate p_cairo_translate
#define cairo_scale p_cairo_scale
#define cairo_set_source_surface p_cairo_set_source_surface
#define cairo_pattern_set_filter p_cairo_pattern_set_filter
#define cairo_get_source p_cairo_get_source
#define cairo_paint p_cairo_paint
#define cairo_surface_status p_cairo_surface_status
#define cairo_status p_cairo_status
#define cairo_restore p_cairo_restore
#define cairo_surface_destroy p_cairo_surface_destroy
#define cairo_status_to_string p_cairo_status_to_string
