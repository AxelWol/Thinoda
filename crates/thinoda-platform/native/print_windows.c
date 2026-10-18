#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <commdlg.h>
#include <cderr.h>
#include <stdlib.h>
#include <string.h>
#include "print.h"

static int win_error(char *error, size_t len, const char *stage, DWORD code) {
    snprintf(error, len, "%s (Windows error %lu)", stage, (unsigned long)code);
    return -1;
}

static int job_error(char *error, size_t len, const char *stage, int result, DWORD code) {
    if (result == SP_USERABORT || code == ERROR_CANCELLED || code == ERROR_PRINT_CANCELLED)
        return 0;
    snprintf(error, len, "%s (GDI status %d, Windows error %lu)", stage, result, (unsigned long)code);
    return -1;
}

int thinoda_print(void *owner, int owner_kind, const unsigned char *pixels,
                  int width, int height, double width_mm, double height_mm,
                  int dpi, const char *title, char *error, size_t error_len) {
    (void)dpi; /* Source pixels already encode source DPI. Driver controls device DPI. */
    HWND window = (HWND)owner;
    if (owner_kind != 1 || !IsWindow(window) ||
        GetWindowThreadProcessId(window, NULL) != GetCurrentThreadId())
        return print_error(error, error_len, "Printing must run on the application window thread");

    PRINTDLGW dialog;
    memset(&dialog, 0, sizeof(dialog));
    dialog.lStructSize = sizeof(dialog);
    dialog.hwndOwner = window;
    dialog.Flags = PD_RETURNDEFAULT;
    /* Query the default DEVMODE only to seed physical page setup, never submit.
     * With no default, still show printer selection instead of hiding the dialog. */
    int defaults = PrintDlgW(&dialog);
    if (!defaults) {
        DWORD code = CommDlgExtendedError();
        if (code != PDERR_NODEFAULTPRN) {
            if (dialog.hDevMode) GlobalFree(dialog.hDevMode);
            if (dialog.hDevNames) GlobalFree(dialog.hDevNames);
            return win_error(error, error_len, "Cannot query printer defaults", code);
        }
        if (dialog.hDevMode) GlobalFree(dialog.hDevMode);
        if (dialog.hDevNames) GlobalFree(dialog.hDevNames);
        dialog.hDevMode = dialog.hDevNames = NULL;
    }
    if (defaults && (!dialog.hDevMode || !dialog.hDevNames)) {
        if (dialog.hDevMode) GlobalFree(dialog.hDevMode);
        if (dialog.hDevNames) GlobalFree(dialog.hDevNames);
        return print_error(error, error_len, "Printer defaults did not contain device settings");
    }
    if (defaults && dialog.hDevMode) {
        DEVMODEW *mode = (DEVMODEW *)GlobalLock(dialog.hDevMode);
        if (!mode) {
            DWORD code = GetLastError();
            if (dialog.hDevMode) GlobalFree(dialog.hDevMode);
            if (dialog.hDevNames) GlobalFree(dialog.hDevNames);
            return win_error(error, error_len, "Cannot access printer settings", code);
        }
        mode->dmFields |= DM_ORIENTATION | DM_PAPERSIZE;
        mode->dmOrientation = width_mm > height_mm ? DMORIENT_LANDSCAPE : DMORIENT_PORTRAIT;
        double short_mm = fmin(width_mm, height_mm);
        double long_mm = fmax(width_mm, height_mm);
        if (fabs(short_mm - 210.0) < 0.1 && fabs(long_mm - 297.0) < 0.1)
            mode->dmPaperSize = DMPAPER_A4;
        else if (fabs(short_mm - 297.0) < 0.1 && fabs(long_mm - 420.0) < 0.1)
            mode->dmPaperSize = DMPAPER_A3;
        else if (fabs(long_mm - 355.6) < 0.1)
            mode->dmPaperSize = DMPAPER_LEGAL;
        else
            mode->dmPaperSize = DMPAPER_LETTER;
        mode->dmFields &= ~(DM_PAPERWIDTH | DM_PAPERLENGTH);
        GlobalUnlock(dialog.hDevMode);
    }
    dialog.Flags = PD_RETURNDC | PD_USEDEVMODECOPIESANDCOLLATE |
                   PD_NOPAGENUMS | PD_NOSELECTION | PD_HIDEPRINTTOFILE;
    dialog.nMinPage = dialog.nMaxPage = dialog.nFromPage = dialog.nToPage = 1;
    dialog.nCopies = 1;
    int result = -1;
    if (!PrintDlgW(&dialog)) {
        DWORD code = CommDlgExtendedError();
        result = code ? win_error(error, error_len, "Cannot open printer dialog", code) : 0;
        goto cleanup;
    }
    if (!dialog.hDC) {
        result = print_error(error, error_len, "Printer dialog returned no device context");
        goto cleanup;
    }
    HDC dc = dialog.hDC;
    int xdpi = GetDeviceCaps(dc, LOGPIXELSX), ydpi = GetDeviceCaps(dc, LOGPIXELSY);
    int area_w = GetDeviceCaps(dc, HORZRES), area_h = GetDeviceCaps(dc, VERTRES);
    if (xdpi <= 0 || ydpi <= 0 || area_w <= 0 || area_h <= 0) {
        result = print_error(error, error_len, "Printer returned invalid printable dimensions");
        goto cleanup;
    }
    /* Use physical units: printers can have different horizontal/vertical DPI. */
    double placement[4];
    if (!thinoda_print_placement(width_mm, height_mm,
                                area_w * 25.4 / xdpi, area_h * 25.4 / ydpi, placement)) {
        result = print_error(error, error_len, "Cannot fit page into printer dimensions");
        goto cleanup;
    }
    int out_w = (int)floor(placement[2] * xdpi / 25.4);
    int out_h = (int)floor(placement[3] * ydpi / 25.4);
    int title_len = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, title, -1, NULL, 0);
    WCHAR *wide_title = title_len > 0 ? malloc((size_t)title_len * sizeof(WCHAR)) : NULL;
    if (!wide_title || !MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, title, -1, wide_title, title_len)) {
        free(wide_title);
        result = print_error(error, error_len, "Cannot encode print job title");
        goto cleanup;
    }
    DOCINFOW document;
    memset(&document, 0, sizeof(document));
    document.cbSize = sizeof(document);
    document.lpszDocName = wide_title;
    SetLastError(ERROR_SUCCESS);
    int started = StartDocW(dc, &document);
    DWORD start_error = started <= 0 ? GetLastError() : ERROR_SUCCESS;
    free(wide_title);
    if (started <= 0) {
        result = job_error(error, error_len, "Cannot start print job", started, start_error);
        goto cleanup;
    }
    SetLastError(ERROR_SUCCESS);
    int page_started = StartPage(dc);
    if (page_started <= 0) {
        result = job_error(error, error_len, "Cannot start printer page", page_started, GetLastError());
        AbortDoc(dc);
        goto cleanup;
    }
    BITMAPINFO bitmap;
    memset(&bitmap, 0, sizeof(bitmap));
    bitmap.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
    bitmap.bmiHeader.biWidth = width;
    bitmap.bmiHeader.biHeight = -height;
    bitmap.bmiHeader.biPlanes = 1;
    bitmap.bmiHeader.biBitCount = 32;
    bitmap.bmiHeader.biCompression = BI_RGB;
    if (!SetStretchBltMode(dc, HALFTONE) || !SetBrushOrgEx(dc, 0, 0, NULL)) {
        result = win_error(error, error_len, "Cannot configure printer raster", GetLastError());
        AbortDoc(dc);
        goto cleanup;
    }
    SetLastError(ERROR_SUCCESS);
    int lines = StretchDIBits(dc, (area_w - out_w) / 2, (area_h - out_h) / 2,
                             out_w, out_h, 0, 0, width, height, pixels,
                             &bitmap, DIB_RGB_COLORS, SRCCOPY);
    if (lines == 0 || lines == GDI_ERROR) {
        result = win_error(error, error_len, "Cannot render printer page", GetLastError());
        AbortDoc(dc);
        goto cleanup;
    }
    SetLastError(ERROR_SUCCESS);
    int page_ended = EndPage(dc);
    if (page_ended <= 0) {
        result = job_error(error, error_len, "Cannot finish printer page", page_ended, GetLastError());
        AbortDoc(dc);
        goto cleanup;
    }
    SetLastError(ERROR_SUCCESS);
    int ended = EndDoc(dc);
    if (ended <= 0) {
        result = job_error(error, error_len, "Cannot submit print job", ended, GetLastError());
        AbortDoc(dc);
        goto cleanup;
    }
    result = 1;
cleanup:
    if (dialog.hDC) DeleteDC(dialog.hDC);
    if (dialog.hDevMode) GlobalFree(dialog.hDevMode);
    if (dialog.hDevNames) GlobalFree(dialog.hDevNames);
    return result;
}
