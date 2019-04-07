@import Cocoa;
@import Metal;
@import QuartzCore;

NS_ASSUME_NONNULL_BEGIN

typedef enum: uint32 {
    NCWindowEventTypeNSEvent = 0,
    NCWindowEventTypeResized = 1,
    NCWindowEventTypeBackingUpdate = 2,
    NCWindowEventTypeWillClose = 3,
    NCWindowEventTypeReady = 4,
} NCWindowEventType;

typedef struct {
    void *window_ptr;
} NCWindowCallbackData;

@interface NCWindowEvent: NSObject
@property (nonatomic) NCWindowEventType eventType;
@property (nonatomic, nullable, retain) NSEvent *event;
@end

@interface NCWindow : NSWindow <NSWindowDelegate> {
    NSMutableArray *events;
    void (*callback)(NCWindow *, BOOL, BOOL);
    BOOL didSendReady;
    BOOL shouldSendReadyOnUpdate;
    unsigned int syncTimeout;
    CVDisplayLinkRef displayLink;
    BOOL displayLinkRunning;
}

@property (readonly, nonatomic) CAMetalLayer *metalLayer;
@property (nonatomic) NCWindowCallbackData callbackData;

- (instancetype)initWithContentRect:(NSRect)contentRect
                           callback:(void (*)(NCWindow*, BOOL, BOOL))callbackFn;
- (NCWindowEvent *)dequeueEvent;
- (void)setDevice:(id<MTLDevice>)device;
- (void)requestFrame;
- (void)handleFrame;
- (NSColorSpace *)layerColorSpace;

@end

NS_ASSUME_NONNULL_END
