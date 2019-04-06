#import "NCAppDelegate.h"

void NCWakeApplication() {
    @autoreleasepool {
        NSEvent *event = [NSEvent otherEventWithType:NSApplicationDefined
                                            location:NSMakePoint(0, 0)
                                       modifierFlags:0
                                           timestamp:0.0
                                        windowNumber:0
                                             context:nil
                                             subtype:0
                                               data1:0
                                               data2:0];
        [NSApp postEvent:event atStart:YES];
    }
}

@implementation NCAppEvent

@synthesize eventType;

- (instancetype) initWithType:(NCAppEventType)type {
    self = [super init];
    self.eventType = type;
    return self;
}

@end

@implementation NCAppDelegate

@synthesize callbackData;

- (instancetype) initWithCallback:(void (*)(NCAppDelegate *))callbackFn {
    self = [super init];
    callback = callbackFn;
    events = [[NSMutableArray alloc] initWithCapacity:2];
    return self;
}

- (void)setDarkAppearance {
    if (IS_MOJAVE_AVAILABLE) {
        // switch to dark aqua on Mojave
        NSApplication *app = [NSApplication sharedApplication];
        // must use a string because it fails to compile otherwise
        [app setAppearance:[NSAppearance appearanceNamed:@"NSAppearanceNameDarkAqua"]];
    } else {
        // switch to graphite
        // (and set windows to vibrantDark on Yosemite)
        [[NSUserDefaults standardUserDefaults] setVolatileDomain:@{@"AppleAquaColorVariant": @6}
                                                         forName:NSArgumentDomain];
    }
}

- (void)doCallback {
    if (callback != nil) {
        callback(self);
    }
    NCWakeApplication();
}

- (void)pushAppEvent:(NCAppEventType)eventType {
    [events addObject:[[NCAppEvent alloc] initWithType:eventType]];
    [self doCallback];
}

- (NCAppEvent *)dequeueEvent {
    if ([events count] == 0) {
        return nil;
    }
    NCAppEvent *event = [events firstObject];
    [events removeObjectAtIndex:0];
    return event;
}

- (void)setDefaultMainMenu:(NSString *)name {
    @autoreleasepool {
        NSMenu *menu = [[NSMenu alloc] initWithTitle:name];
        NSMenuItem *appMenuItem = [[NSMenuItem alloc] initWithTitle:name
                                                             action:nil
                                                      keyEquivalent:@""];
        NSMenu *appMenu = [[NSMenu alloc] initWithTitle:name];
        NSString *aboutTitle = [NSString stringWithFormat:@"About %@", name];
        NSMenuItem *about = [[NSMenuItem alloc] initWithTitle:aboutTitle
                                                       action:@selector(orderFrontStandardAboutPanel:)
                                                keyEquivalent:@""];
        NSMenuItem *quitApp = [[NSMenuItem alloc] initWithTitle:@"Quit"
                                                         action:@selector(terminate:)
                                                  keyEquivalent:@"q"];
        [appMenu addItem:about];
        [appMenu addItem:[NSMenuItem separatorItem]];
        [appMenu addItem:quitApp];
        [appMenuItem setSubmenu:appMenu];
        [menu addItem:appMenuItem];
        [[NSApplication sharedApplication] setMainMenu:menu];
    }
}

- (void)applicationDidFinishLaunching:(NSNotification *)notification {
    [self pushAppEvent:NCAppEventTypeReady];
}

- (void)applicationWillTerminate:(NSNotification *)notification {
    [self pushAppEvent:NCAppEventTypeTerminating];
}

@end
