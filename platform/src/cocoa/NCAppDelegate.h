@import Cocoa;

NS_ASSUME_NONNULL_BEGIN

// there’s no NSAppKitVersionNumber10_14 constant so here’s `NSAppKitVersion.current` from
// a Swift Playground run on Mojave
#define IS_MOJAVE_AVAILABLE (NSAppKitVersionNumber >= 1671.0)

typedef enum : uint32 {
    NCAppEventTypeReady = 0,
    NCAppEventTypeTerminating = 1,
} NCAppEventType;

typedef struct {
    void *app_ptr;
} NCAppDelegateCallbackData;

void NCWakeApplication();

@interface NCAppEvent : NSObject
@property (nonatomic) NCAppEventType eventType;
@end

@interface NCAppDelegate : NSObject<NSApplicationDelegate> {
    NSMutableArray *events;
    void (*callback)(NCAppDelegate *);
}

@property (nonatomic) NCAppDelegateCallbackData callbackData;

- (instancetype) initWithCallback:(void (*)(NCAppDelegate*))callbackFn;
- (void) setDarkAppearance;
- (void) setDefaultMainMenu:(NSString *)name;
- (NCAppEvent *) dequeueEvent;

@end

NS_ASSUME_NONNULL_END
