#import <AppKit/AppKit.h>
#include <stdint.h>
#include <stdio.h>

typedef void (*ThinodaMenuWake)(void *);

@interface ThinodaMenu : NSObject <NSMenuDelegate>
@property(strong) NSMenu *root;
@property(strong) NSMenu *previous;
@property(strong) NSMenu *previousServices;
@property(strong) NSMenu *previousWindows;
@property(strong) NSMenu *previousHelp;
@property(strong) NSMutableArray<NSNumber *> *pending;
@property(strong) NSSet<NSNumber *> *allowed;
@property NSUInteger tracking;
@property NSUInteger generation;
@property ThinodaMenuWake wake;
@property void *context;
@end

@implementation ThinodaMenu
- (void)menuWillOpen:(NSMenu *)menu {
    (void)menu;
    self.tracking += 1;
}
- (void)menuDidClose:(NSMenu *)menu {
    (void)menu;
    if (self.tracking) self.tracking -= 1;
    if (self.wake) self.wake(self.context);
}
- (void)activate:(NSMenuItem *)item {
    NSDictionary *target = item.representedObject;
    NSNumber *identifier = target[@"id"];
    if ([target[@"generation"] unsignedIntegerValue] != self.generation ||
        !item.enabled || ![self.allowed containsObject:identifier]) return;
    [self.pending addObject:identifier];
    if (self.wake) self.wake(self.context);
}
- (BOOL)validateMenuItem:(NSMenuItem *)item {
    NSDictionary *target = item.representedObject;
    return [target[@"generation"] unsignedIntegerValue] == self.generation &&
        [self.allowed containsObject:target[@"id"]];
}
@end

static int failure(char *error, size_t length, NSString *message) {
    snprintf(error, length, "%s", message.UTF8String);
    return -1;
}

static NSMenu *new_menu(ThinodaMenu *owner, NSString *title) {
    NSMenu *menu = [[NSMenu alloc] initWithTitle:title];
    menu.autoenablesItems = NO;
    menu.delegate = owner;
    return menu;
}

static NSDictionary *find_action(NSArray *entries, NSNumber *identifier) {
    for (NSDictionary *entry in entries) {
        if ([entry[@"id"] isEqual:identifier]) return entry;
        NSDictionary *found = find_action(entry[@"items"], identifier);
        if (found) return found;
    }
    return nil;
}

static NSMenuItem *action_item(ThinodaMenu *owner, NSDictionary *entry) {
    NSMenuItem *item = [[NSMenuItem alloc] initWithTitle:entry[@"label"]
        action:@selector(activate:) keyEquivalent:entry[@"key"]];
    item.target = owner;
    item.enabled = [entry[@"enabled"] boolValue];
    item.state = [entry[@"checked"] boolValue] ? NSControlStateValueOn : NSControlStateValueOff;
    item.keyEquivalentModifierMask = [entry[@"modifiers"] unsignedIntegerValue];
    item.representedObject = @{@"id": entry[@"id"], @"generation": @(owner.generation + 1)};
    return item;
}

static void append_entries(ThinodaMenu *owner, NSMenu *parent, NSArray *entries,
                          NSSet *aliases, NSMutableSet *allowed, BOOL ancestorsEnabled) {
    for (NSDictionary *entry in entries) {
        if ([entry[@"separator"] boolValue]) {
            [parent addItem:NSMenuItem.separatorItem];
        } else if (entry[@"items"]) {
            NSMenuItem *item = [[NSMenuItem alloc] initWithTitle:entry[@"label"] action:nil keyEquivalent:@""];
            item.enabled = [entry[@"enabled"] boolValue];
            item.submenu = new_menu(owner, entry[@"label"]);
            append_entries(owner, item.submenu, entry[@"items"], aliases, allowed, ancestorsEnabled && item.enabled);
            [parent addItem:item];
        } else if (entry[@"id"]) {
            if ([aliases containsObject:entry[@"id"]]) continue;
            NSMenuItem *item = action_item(owner, entry);
            [parent addItem:item];
            if (ancestorsEnabled && item.enabled) [allowed addObject:entry[@"id"]];
        } else {
            NSMenuItem *item = [[NSMenuItem alloc] initWithTitle:entry[@"label"] action:nil keyEquivalent:@""];
            item.enabled = NO;
            [parent addItem:item];
        }
    }
}

static NSMenuItem *system_item(NSString *title, SEL action, id target, NSString *key,
                                NSEventModifierFlags modifiers) {
    NSMenuItem *item = [[NSMenuItem alloc] initWithTitle:title action:action keyEquivalent:key];
    item.target = target;
    item.keyEquivalentModifierMask = modifiers;
    return item;
}

static int install(ThinodaMenu *owner, const char *json, char *error, size_t length) {
    NSData *data = [[NSString stringWithUTF8String:json] dataUsingEncoding:NSUTF8StringEncoding];
    NSError *parseError = nil;
    NSDictionary *payload = [NSJSONSerialization JSONObjectWithData:data options:0 error:&parseError];
    if (!payload || parseError) return failure(error, length, parseError.localizedDescription ?: @"Invalid native menu data");
    NSArray *entries = payload[@"items"];
    NSDictionary *labels = payload[@"labels"];
    NSArray *commands = payload[@"application_commands"];
    NSNumber *closeCommand = payload[@"close_window_command"];
    NSDictionary *closeEntry = find_action(entries, closeCommand);
    if (!closeEntry || [commands containsObject:closeCommand])
        return failure(error, length, @"Missing or conflicting Close Window command");
    NSMutableSet *allowed = [NSMutableSet set];
    NSMutableSet *aliases = [NSMutableSet setWithArray:commands];
    [aliases addObject:closeCommand];
    NSMenu *root = new_menu(owner, @"");
    NSMenu *application = new_menu(owner, labels[@"application"]);
    NSMenu *services = new_menu(owner, labels[@"services"]);
    services.autoenablesItems = YES;
    NSMenuItem *appItem = [[NSMenuItem alloc] initWithTitle:labels[@"application"] action:nil keyEquivalent:@""];
    appItem.submenu = application;
    [root addItem:appItem];
    for (NSUInteger index = 0; index < commands.count; ++index) {
        NSDictionary *entry = find_action(entries, commands[index]);
        if (!entry) return failure(error, length, @"Missing application menu command");
        NSMenuItem *item = action_item(owner, entry);
        [application addItem:item];
        if (item.enabled) [allowed addObject:commands[index]];
        if (index == 0 || index == 1) [application addItem:NSMenuItem.separatorItem];
        if (index == 1) {
            NSMenuItem *serviceItem = [[NSMenuItem alloc] initWithTitle:labels[@"services"] action:nil keyEquivalent:@""];
            serviceItem.submenu = services;
            [application addItem:serviceItem];
            [application addItem:NSMenuItem.separatorItem];
            [application addItem:system_item(labels[@"hide"], @selector(hide:), NSApp, @"h", NSEventModifierFlagCommand)];
            [application addItem:system_item(labels[@"hide_others"], @selector(hideOtherApplications:), NSApp, @"h",
                NSEventModifierFlagCommand | NSEventModifierFlagOption)];
            [application addItem:system_item(labels[@"show_all"], @selector(unhideAllApplications:), NSApp, @"", 0)];
            [application addItem:NSMenuItem.separatorItem];
        }
    }
    append_entries(owner, root, entries, aliases, allowed, YES);
    NSMenu *windows = new_menu(owner, labels[@"window"]);
    windows.autoenablesItems = YES;
    [windows addItem:action_item(owner, closeEntry)];
    if ([closeEntry[@"enabled"] boolValue]) [allowed addObject:closeCommand];
    [windows addItem:NSMenuItem.separatorItem];
    [windows addItem:system_item(labels[@"minimize"], @selector(performMiniaturize:), nil, @"m", NSEventModifierFlagCommand)];
    [windows addItem:system_item(labels[@"zoom"], @selector(performZoom:), nil, @"", 0)];
    [windows addItem:NSMenuItem.separatorItem];
    [windows addItem:system_item(labels[@"bring_all"], @selector(arrangeInFront:), NSApp, @"", 0)];
    NSMenuItem *windowItem = [[NSMenuItem alloc] initWithTitle:labels[@"window"] action:nil keyEquivalent:@""];
    windowItem.submenu = windows;
    [root insertItem:windowItem atIndex:MAX(1, root.numberOfItems - 1)];
    // Swap complete trees only outside menu tracking, after all queued IDs were drained.
    owner.generation += 1;
    owner.allowed = allowed;
    owner.root = root;
    NSApp.mainMenu = root;
    NSApp.servicesMenu = services;
    NSApp.windowsMenu = windows;
    NSApp.helpMenu = root.itemArray.lastObject.submenu;
    return 1;
}

int thinoda_menu_attach(const char *json, ThinodaMenuWake wake, void *context,
                        void **result, char *error, size_t length) {
    if (![NSThread isMainThread] || !NSApp)
        return failure(error, length, @"Native menus require the AppKit main thread");
    @autoreleasepool {
        ThinodaMenu *owner = [ThinodaMenu new];
        owner.previous = NSApp.mainMenu;
        owner.previousServices = NSApp.servicesMenu;
        owner.previousWindows = NSApp.windowsMenu;
        owner.previousHelp = NSApp.helpMenu;
        owner.pending = [NSMutableArray array];
        owner.wake = wake;
        owner.context = context;
        int status = install(owner, json, error, length);
        if (status < 0) return status;
        *result = (__bridge_retained void *)owner;
        return 1;
    }
}

int thinoda_menu_replace(void *handle, const char *json, char *error, size_t length) {
    if (![NSThread isMainThread]) return failure(error, length, @"Native menus require the AppKit main thread");
    @autoreleasepool {
        ThinodaMenu *owner = (__bridge ThinodaMenu *)handle;
        if (owner.tracking || owner.pending.count) return 0;
        return install(owner, json, error, length);
    }
}

size_t thinoda_menu_drain(void *handle, uint16_t *output, size_t capacity) {
    ThinodaMenu *owner = (__bridge ThinodaMenu *)handle;
    NSUInteger count = MIN(capacity, owner.pending.count);
    for (NSUInteger i = 0; i < count; ++i) output[i] = owner.pending[i].unsignedShortValue;
    [owner.pending removeObjectsInRange:NSMakeRange(0, count)];
    return count;
}

void thinoda_menu_detach(void *handle) {
    @autoreleasepool {
        ThinodaMenu *owner = (__bridge_transfer ThinodaMenu *)handle;
        owner.wake = NULL;
        owner.context = NULL;
        if (NSApp.mainMenu == owner.root) {
            [owner.root cancelTracking];
            NSApp.mainMenu = owner.previous;
            NSApp.servicesMenu = owner.previousServices;
            NSApp.windowsMenu = owner.previousWindows;
            NSApp.helpMenu = owner.previousHelp;
        }
    }
}
