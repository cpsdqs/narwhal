@import Cocoa;

NS_ASSUME_NONNULL_BEGIN

typedef enum : uint32 {
    NCAppEventTypeReady = 0,
    NCAppEventTypeTerminating = 1,
} NCAppEventType;

typedef struct {
    void *app_ptr;
} NCAppDelegateCallbackData;

@interface NCAppEvent : NSObject
@property (nonatomic) NCAppEventType eventType;
@end

@interface NCAppDelegate : NSObject<NSApplicationDelegate> {
    NSMutableArray *events;
    void (*callback)(NCAppDelegate *);
}

@property (nonatomic) NCAppDelegateCallbackData callbackData;

+ (BOOL) isMetalAvailable;

- (instancetype) initWithCallback:(void (*)(NCAppDelegate*))callbackFn;
- (void) setDarkAppearance;
- (void) setDefaultMainMenu:(NSString *)name;
- (NSArray *) drainEvents;

@end

NS_ASSUME_NONNULL_END
