#import "../menu_macos.m"
#include <assert.h>

static void wake_test(void *context) {
    *(int *)context += 1;
}

int main(void) {
    @autoreleasepool {
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyProhibited];
        NSMenu *previous = [[NSMenu alloc] initWithTitle:@"Previous"];
        NSApp.mainMenu = previous;
        NSDictionary *action = @{@"id":@4, @"label":@"Save", @"enabled":@YES,
            @"checked":@YES, @"key":@"s", @"modifiers":@(NSEventModifierFlagCommand)};
        NSDictionary *disabledChild = @{@"id":@5, @"label":@"Unavailable child",
            @"enabled":@YES, @"checked":@NO, @"key":@"", @"modifiers":@0};
        NSDictionary *disabledGroup = @{@"label":@"Disabled group", @"enabled":@NO, @"items":@[disabledChild]};
        NSMutableArray *fileEntries = [NSMutableArray array];
        for (NSNumber *identifier in @[@1, @2, @3]) {
            [fileEntries addObject:@{@"id":identifier, @"label":@"Application command",
                @"enabled":@YES, @"checked":@NO,
                @"key":[identifier isEqual:@2] ? @"p" : @"",
                @"modifiers":[identifier isEqual:@2] ? @(NSEventModifierFlagCommand | NSEventModifierFlagOption) : @0}];
        }
        NSDictionary *close = @{@"id":@6, @"label":@"Close Window", @"enabled":@YES,
            @"checked":@NO, @"key":@"k", @"modifiers":@(NSEventModifierFlagCommand | NSEventModifierFlagOption)};
        [fileEntries addObjectsFromArray:@[action, disabledGroup, close]];
        NSDictionary *payload = @{
            @"items":@[
                @{@"label":@"File", @"enabled":@YES, @"items":fileEntries},
                @{@"label":@"Help", @"enabled":@YES, @"items":@[]}
            ],
            @"application_commands":@[@1,@2,@3],
            @"close_window_command":@6,
            @"labels":@{@"application":@"Thinoda Test", @"services":@"Services", @"window":@"Window",
                @"hide":@"Hide Thinoda", @"hide_others":@"Hide Others", @"show_all":@"Show All",
                @"minimize":@"Minimize", @"zoom":@"Zoom", @"bring_all":@"Bring All to Front"}
        };
        NSData *data = [NSJSONSerialization dataWithJSONObject:payload options:0 error:NULL];
        NSString *json = [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding];
        char error[512] = {0};
        void *handle = NULL;
        int woken = 0;
        assert(thinoda_menu_attach(json.UTF8String, wake_test, &woken, &handle, error, sizeof(error)) == 1);
        ThinodaMenu *owner = (__bridge ThinodaMenu *)handle;
        assert(NSApp.mainMenu == owner.root);
        assert(NSApp.mainMenu.numberOfItems == 4);
        assert(NSApp.servicesMenu && NSApp.windowsMenu && NSApp.helpMenu);
        NSMenuItem *closeWindow = NSApp.windowsMenu.itemArray[0];
        assert([closeWindow.title isEqualToString:@"Close Window"]);
        assert(closeWindow.action == @selector(activate:));
        assert(closeWindow.target == owner);
        assert([closeWindow.keyEquivalent isEqualToString:@"k"]);
        assert(closeWindow.keyEquivalentModifierMask == (NSEventModifierFlagCommand | NSEventModifierFlagOption));
        NSMenu *application = NSApp.mainMenu.itemArray[0].submenu;
        assert([application.itemArray[2].keyEquivalent isEqualToString:@"p"]);
        assert(application.itemArray[2].keyEquivalentModifierMask ==
            (NSEventModifierFlagCommand | NSEventModifierFlagOption));
        assert([application.itemArray.lastObject.keyEquivalent isEqualToString:@""]);
        NSMenu *file = NSApp.mainMenu.itemArray[1].submenu;
        NSMenuItem *stale = file.itemArray[0];
        assert(stale.state == NSControlStateValueOn);
        assert([stale.keyEquivalent isEqualToString:@"s"]);
        [file performActionForItemAtIndex:0];
        assert(woken == 1);
        assert(thinoda_menu_replace(handle, json.UTF8String, error, sizeof(error)) == 0);
        uint16_t selected[4] = {0};
        assert(thinoda_menu_drain(handle, selected, 4) == 1 && selected[0] == 4);
        [NSApp.windowsMenu performActionForItemAtIndex:0];
        assert(thinoda_menu_drain(handle, selected, 4) == 1 && selected[0] == 6);
        [owner activate:file.itemArray[1].submenu.itemArray[0]];
        assert(thinoda_menu_drain(handle, selected, 4) == 0);
        [owner menuWillOpen:file];
        assert(thinoda_menu_replace(handle, json.UTF8String, error, sizeof(error)) == 0);
        [owner menuDidClose:file];
        assert(thinoda_menu_replace(handle, json.UTF8String, error, sizeof(error)) == 1);
        [owner activate:stale];
        assert(thinoda_menu_drain(handle, selected, 4) == 0);
        assert(thinoda_menu_replace(handle, "{", error, sizeof(error)) == -1);
        assert(NSApp.mainMenu == owner.root);
        thinoda_menu_detach(handle);
        assert(NSApp.mainMenu == previous);
        assert(owner.wake == NULL && owner.context == NULL);
    }
    return 0;
}
