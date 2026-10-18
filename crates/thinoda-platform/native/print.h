/* Private synchronous ABI. Input is bounded opaque top-down BGRA, borrowed for
 * this call only. 0 = cancelled, 1 = OS accepted, -1 = error (UTF-8 message).
 * All backends print one page; no default-printer or external-app fallback. */
#include <stddef.h>
#include <stdio.h>
#include <math.h>

static int print_error(char *error, size_t len, const char *message) {
    snprintf(error, len, "%s", message);
    return -1;
}

/* Physical, uniform fit of the entire preview page inside a printable rectangle.
 * Margins are already part of the pixels. Never independently stretch X and Y.
 * Units can be points, mm or inches; both axes must use the same physical unit. */
#ifdef __cplusplus
extern "C"
#endif
int thinoda_print_placement(double page_w, double page_h, double area_w, double area_h,
                           double out[4]) {
    if (!isfinite(page_w) || !isfinite(page_h) || !isfinite(area_w) || !isfinite(area_h)
        || page_w <= 0 || page_h <= 0 || area_w <= 0 || area_h <= 0) return 0;
    double scale = fmin(area_w / page_w, area_h / page_h);
    out[2] = page_w * scale;
    out[3] = page_h * scale;
    out[0] = (area_w - out[2]) / 2;
    out[1] = (area_h - out[3]) / 2;
    return 1;
}
