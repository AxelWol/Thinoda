#include <gtk/gtk.h>
#include <unistd.h>
#include <sys/syscall.h>
#include "print.h"
#include "gtk_runtime.h"

typedef struct {
    const unsigned char *pixels;
    int width, height;
    double width_mm, height_mm;
    cairo_status_t draw_error;
} PrintData;

static void begin_print(GtkPrintOperation *operation, GtkPrintContext *context, gpointer user) {
    (void)context;
    (void)user;
    gtk_print_operation_set_n_pages(operation, 1);
}

static void draw_page(GtkPrintOperation *operation, GtkPrintContext *context, gint page, gpointer user) {
    (void)page;
    PrintData *data = user;
    cairo_t *cr = gtk_print_context_get_cairo_context(context);
    GtkPageSetup *setup = gtk_print_context_get_page_setup(context);
    double top = gtk_page_setup_get_top_margin(setup, GTK_UNIT_MM);
    double bottom = gtk_page_setup_get_bottom_margin(setup, GTK_UNIT_MM);
    double left = gtk_page_setup_get_left_margin(setup, GTK_UNIT_MM);
    double right = gtk_page_setup_get_right_margin(setup, GTK_UNIT_MM);
    double hard_top, hard_bottom, hard_left, hard_right;
    if (gtk_print_context_get_hard_margins(context, &hard_top, &hard_bottom, &hard_left, &hard_right)) {
        top = fmax(top, hard_top);
        bottom = fmax(bottom, hard_bottom);
        left = fmax(left, hard_left);
        right = fmax(right, hard_right);
    }
    /* With full_page=TRUE the context is paper-sized and has a paper origin.
     * Use the selected page setup and, where reported, hardware margins. GTK
     * virtual-file backends may not report hardware margins (none exist). */
    double area_w = gtk_print_context_get_width(context) - left - right;
    double area_h = gtk_print_context_get_height(context) - top - bottom;
    double placement[4];
    if (!thinoda_print_placement(data->width_mm, data->height_mm, area_w, area_h, placement)) {
        data->draw_error = CAIRO_STATUS_INVALID_SIZE;
        gtk_print_operation_cancel(operation);
        return;
    }
    cairo_surface_t *surface = cairo_image_surface_create_for_data(
        (unsigned char *)data->pixels, CAIRO_FORMAT_RGB24,
        data->width, data->height, data->width * 4);
    cairo_save(cr);
    cairo_translate(cr, left + placement[0], top + placement[1]);
    cairo_scale(cr, placement[2] / data->width, placement[3] / data->height);
    cairo_set_source_surface(cr, surface, 0, 0);
    cairo_pattern_set_filter(cairo_get_source(cr), CAIRO_FILTER_BEST);
    cairo_paint(cr);
    data->draw_error = cairo_surface_status(surface);
    if (data->draw_error == CAIRO_STATUS_SUCCESS) data->draw_error = cairo_status(cr);
    cairo_restore(cr);
    cairo_surface_destroy(surface);
    if (data->draw_error != CAIRO_STATUS_SUCCESS) gtk_print_operation_cancel(operation);
}

int thinoda_print(void *owner, int owner_kind, const unsigned char *pixels,
                  int width, int height, double width_mm, double height_mm,
                  int dpi, const char *title, char *error, size_t error_len) {
    (void)dpi;
    (void)owner;
    /* GTK and the winit event loop must share the actual process main thread. */
    if (syscall(SYS_gettid) != getpid())
        return print_error(error, error_len, "GTK printing must run on the main thread");
    if (owner_kind != 3 && owner_kind != 4)
        return print_error(error, error_len, "Unsupported GTK print window system");
    if (!load_gtk(error, error_len)) return -1;
    if (!gtk_init_check(NULL, NULL))
        return print_error(error, error_len, "Cannot initialize GTK printing; check the desktop display");

    GtkPrintOperation *operation = gtk_print_operation_new();
    GtkPageSetup *setup = gtk_page_setup_new();
    double short_mm = fmin(width_mm, height_mm), long_mm = fmax(width_mm, height_mm);
    const char *paper_name;
    if (fabs(short_mm - 210.0) < 0.1 && fabs(long_mm - 297.0) < 0.1)
        paper_name = GTK_PAPER_NAME_A4;
    else if (fabs(short_mm - 297.0) < 0.1 && fabs(long_mm - 420.0) < 0.1)
        paper_name = GTK_PAPER_NAME_A3;
    else if (fabs(long_mm - 355.6) < 0.1)
        paper_name = GTK_PAPER_NAME_LEGAL;
    else
        paper_name = GTK_PAPER_NAME_LETTER;
    GtkPaperSize *paper = gtk_paper_size_new(paper_name);
    gtk_page_setup_set_paper_size(setup, paper);
    gtk_paper_size_free(paper);
    gtk_page_setup_set_orientation(setup, width_mm > height_mm
        ? GTK_PAGE_ORIENTATION_LANDSCAPE : GTK_PAGE_ORIENTATION_PORTRAIT);
    /* Start without GTK's arbitrary default document margins (ours are already
     * in the pixels). draw_page also respects reported hardware margins and any
     * page-setup margins the user changes in the system dialog. */
    gtk_page_setup_set_top_margin(setup, 0, GTK_UNIT_MM);
    gtk_page_setup_set_bottom_margin(setup, 0, GTK_UNIT_MM);
    gtk_page_setup_set_left_margin(setup, 0, GTK_UNIT_MM);
    gtk_page_setup_set_right_margin(setup, 0, GTK_UNIT_MM);
    gtk_print_operation_set_default_page_setup(operation, setup);
    g_object_unref(setup);
    gtk_print_operation_set_job_name(operation, title);
    gtk_print_operation_set_unit(operation, GTK_UNIT_MM);
    gtk_print_operation_set_use_full_page(operation, TRUE);
    gtk_print_operation_set_embed_page_setup(operation, TRUE);
    gtk_print_operation_set_allow_async(operation, FALSE);
    gtk_print_operation_set_show_progress(operation, TRUE);
    PrintData data = {pixels, width, height, width_mm, height_mm, CAIRO_STATUS_SUCCESS};
    g_signal_connect(operation, "begin-print", G_CALLBACK(begin_print), &data);
    g_signal_connect(operation, "draw-page", G_CALLBACK(draw_page), &data);
    GError *native_error = NULL;
    /* winit is not GTK: its Wayland surface is not a GtkWindow. The native
     * dialog is application-modal and the winit UI loop remains blocked here;
     * no foreign pointer is ever passed as a GtkWindow. */
    GtkPrintOperationResult result = gtk_print_operation_run(
        operation, GTK_PRINT_OPERATION_ACTION_PRINT_DIALOG, NULL, &native_error);
    int outcome;
    if (data.draw_error != CAIRO_STATUS_SUCCESS)
        outcome = print_error(error, error_len, cairo_status_to_string(data.draw_error));
    else if (result == GTK_PRINT_OPERATION_RESULT_ERROR) {
        if (!native_error) gtk_print_operation_get_error(operation, &native_error);
        outcome = print_error(error, error_len, native_error
            ? native_error->message : "GTK print operation failed without an error description");
    } else if (result == GTK_PRINT_OPERATION_RESULT_CANCEL)
        outcome = 0;
    else if (result == GTK_PRINT_OPERATION_RESULT_APPLY)
        outcome = 1;
    else
        outcome = print_error(error, error_len, "GTK unexpectedly started an asynchronous print operation");
    if (native_error) g_error_free(native_error);
    g_object_unref(operation);
    return outcome;
}
