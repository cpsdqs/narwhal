#import "NCAppDelegate.h"
#import "available.h"

@implementation NCAppEvent

@synthesize eventType;

- (instancetype) initWithType:(NCAppEventType)eventType {
    self = [super init];
    self.eventType = eventType;
    return self;
}

@end

@implementation NCAppDelegate

@synthesize callbackData;

+ (BOOL)isMetalAvailable {
    return IS_EL_CAPITAN_AVAILABLE;
}

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

- (void)_doCallback {
    if (callback != nil) {
        callback(self);
    }
}

- (void)pushAppEvent:(NCAppEventType)eventType {
    [events addObject:[[NCAppEvent alloc] initWithType:eventType]];
    [self _doCallback];
}

- (NSArray *)drainEvents {
    NSArray *result = events;
    events = [[NSMutableArray alloc] initWithCapacity:2];
    return result;
}

- (void)setDefaultMainMenu:(NSString *)name {
    NSMenu *menu = [[NSMenu alloc] initWithTitle:name];
    NSMenuItem *appMenuItem = [[NSMenuItem alloc] initWithTitle:name action:nil keyEquivalent:@""];
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

- (void)applicationDidFinishLaunching:(NSNotification *)notification {
    [self pushAppEvent:NCAppEventTypeReady];
}

- (void)applicationWillTerminate:(NSNotification *)notification {
    [self pushAppEvent:NCAppEventTypeTerminating];
}

@end
