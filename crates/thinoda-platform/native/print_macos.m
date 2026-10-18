#import <AppKit/AppKit.h>
#import <ApplicationServices/ApplicationServices.h>
#include <string.h>
#include "print.h"

@interface ThinodaPrintView : NSView
@property(strong) NSImage *pageImage;
@property(strong) NSPrintInfo *pageInfo;
@property NSSize sourceMM;
@property BOOL drawingFailed;
@end

@implementation ThinodaPrintView
- (BOOL)isFlipped { return YES; }
- (BOOL)knowsPageRange:(NSRangePointer)range {
    NSRect area = self.pageInfo.imageablePageBounds;
    NSSize paper = self.pageInfo.paperSize;
    /* Printer-selected imageable bounds, in points, after the native panel. */
    self.pageInfo.leftMargin = NSMinX(area);
    self.pageInfo.rightMargin = paper.width - NSMaxX(area);
    self.pageInfo.bottomMargin = NSMinY(area);
    self.pageInfo.topMargin = paper.height - NSMaxY(area);
    self.frame = NSMakeRect(0, 0, area.size.width, area.size.height);
    *range = NSMakeRange(1, 1);
    return YES;
}
- (NSRect)rectForPage:(NSInteger)page {
    (void)page;
    return self.bounds;
}
- (void)drawRect:(NSRect)dirty {
    (void)dirty;
    NSSize area = self.bounds.size;
    double placement[4];
    if (!thinoda_print_placement(self.sourceMM.width, self.sourceMM.height,
                                 area.width, area.height, placement)) {
        self.drawingFailed = YES;
        return;
    }
    NSRect target = NSMakeRect(placement[0], placement[1], placement[2], placement[3]);
    [self.pageImage drawInRect:target fromRect:NSZeroRect operation:NSCompositingOperationCopy
                      fraction:1.0 respectFlipped:YES hints:@{NSImageHintInterpolation: @(NSImageInterpolationHigh)}];
}
@end

int thinoda_print(void *owner, int owner_kind, const unsigned char *pixels,
                  int width, int height, double width_mm, double height_mm,
                  int dpi, const char *title, char *error, size_t error_len) {
    (void)dpi;
    if (![NSThread isMainThread])
        return print_error(error, error_len, "AppKit printing must run on the main thread");
    if (owner_kind != 2 || !owner || ![(__bridge NSView *)owner window])
        return print_error(error, error_len, "Cannot find the application print window");
    @autoreleasepool {
        NSPrintInfo *info = [NSPrintInfo.sharedPrintInfo copy];
        NSSize requested = NSMakeSize(fmin(width_mm, height_mm) * 72.0 / 25.4,
                                       fmax(width_mm, height_mm) * 72.0 / 25.4);
        /* Prefer the driver's named standard medium over a custom size when
         * it advertises one. Unsupported media remain editable in the panel. */
        NSString *matchedPaper = nil;
        for (NSString *name in info.printer.paperNames) {
            NSSize size = [info.printer pageSizeForPaper:name];
            if (fabs(fmin(size.width, size.height) - requested.width) < 0.5 &&
                fabs(fmax(size.width, size.height) - requested.height) < 0.5) {
                matchedPaper = name;
                break;
            }
        }
        if (matchedPaper) info.paperName = matchedPaper;
        else info.paperSize = requested;
        info.orientation = width_mm > height_mm ? NSPaperOrientationLandscape : NSPaperOrientationPortrait;
        info.horizontalPagination = NSPrintingPaginationModeClip;
        info.verticalPagination = NSPrintingPaginationModeClip;
        info.horizontallyCentered = NO;
        info.verticallyCentered = NO;
        info.scalingFactor = 1.0;
        info.dictionary[NSPrintHeaderAndFooter] = @NO;
        info.dictionary[NSPrintSelectionOnly] = @NO;
        info.dictionary[NSPrintAllPages] = @YES;
        NSBitmapImageRep *bitmap = [[NSBitmapImageRep alloc]
            initWithBitmapDataPlanes:NULL pixelsWide:width pixelsHigh:height
            bitsPerSample:8 samplesPerPixel:4 hasAlpha:YES isPlanar:NO
            colorSpaceName:NSDeviceRGBColorSpace
            bitmapFormat:(NSBitmapFormatAlphaFirst | NSBitmapFormatThirtyTwoBitLittleEndian)
            bytesPerRow:width * 4 bitsPerPixel:32];
        if (!bitmap || !bitmap.bitmapData)
            return print_error(error, error_len, "Cannot allocate native print image");
        memcpy(bitmap.bitmapData, pixels, (size_t)width * (size_t)height * 4);
        NSImage *image = [[NSImage alloc] initWithSize:NSMakeSize(width, height)];
        [image addRepresentation:bitmap];
        ThinodaPrintView *view = [[ThinodaPrintView alloc] initWithFrame:NSMakeRect(0, 0, 1, 1)];
        view.pageImage = image;
        view.sourceMM = NSMakeSize(width_mm, height_mm);
        NSPrintOperation *operation = [NSPrintOperation printOperationWithView:view printInfo:info];
        info = operation.printInfo;
        view.pageInfo = info;
        operation.jobTitle = [NSString stringWithUTF8String:title];
        if (!operation.jobTitle)
            return print_error(error, error_len, "Cannot encode print job title");
        /* Run the panel explicitly so cancellation is not confused with a failed
         * operation (runOperation's NO alone cannot distinguish those cases).
         * This is AppKit's application-modal printer/driver settings panel.
         * Our egui preview remains the document preview; no fake NSView preview. */
        NSPrintPanel *panel = operation.printPanel;
        panel.options = NSPrintPanelShowsCopies | NSPrintPanelShowsPaperSize | NSPrintPanelShowsOrientation;
        NSInteger response = [panel runModalWithPrintInfo:info];
        if (response == NSModalResponseCancel) return 0;
        if (response != NSModalResponseOK)
            return print_error(error, error_len, "AppKit printer dialog returned an unexpected response");
        NSRect area = info.imageablePageBounds;
        NSSize paper = info.paperSize;
        double placement[4];
        if (!isfinite(area.origin.x) || !isfinite(area.origin.y) ||
            !isfinite(paper.width) || !isfinite(paper.height) ||
            paper.width <= 0 || paper.height <= 0 ||
            !thinoda_print_placement(width_mm, height_mm, area.size.width, area.size.height, placement))
            return print_error(error, error_len, "Printer returned invalid printable dimensions");
        operation.showsPrintPanel = NO;
        operation.showsProgressPanel = NO;
        operation.canSpawnSeparateThread = NO;
        if (![operation runOperation]) {
            OSStatus status = PMSessionError((PMPrintSession)[info PMPrintSession]);
            if (status == kPMCancel || status == kPMJobCanceled) return 0;
            snprintf(error, error_len, "AppKit could not submit the print job (PrintCore status %d)", (int)status);
            return -1;
        }
        if (view.drawingFailed)
            return print_error(error, error_len, "AppKit failed to draw the print page");
        return 1;
    }
}
