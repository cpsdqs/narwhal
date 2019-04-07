#import "NCAppDelegate.h"
#import "NCWindow.h"

@implementation NCWindowEvent
@synthesize eventType;
@synthesize event;

- (NSString *)description
{
    return [NSString stringWithFormat:@"<%@: %p> %@", [self class], self, self.event];
}
@end

CVReturn displayLinkCallback(
    CVDisplayLinkRef displayLink,
    const CVTimeStamp *inNow,
    const CVTimeStamp *inOutputTime,
    CVOptionFlags flagsIn,
    CVOptionFlags *flagsOut,
    void *displayLinkContext
) {
    NCWindow *window = (__bridge NCWindow *) displayLinkContext;
    [window handleFrame];
    return kCVReturnSuccess;
}

@implementation NCWindow

@synthesize metalLayer;
@synthesize callbackData;

- (instancetype)initWithContentRect:(NSRect)contentRect
                           callback:(void (*)(NCWindow*, BOOL, BOOL))callbackFn {
    self = [super initWithContentRect:contentRect
                     styleMask:NSWindowStyleMaskTitled
                             | NSWindowStyleMaskClosable
                             | NSWindowStyleMaskMiniaturizable
                             | NSWindowStyleMaskResizable
                       backing:NSBackingStoreBuffered
                         defer:NO];

    callback = callbackFn;
    events = [[NSMutableArray alloc] initWithCapacity:2];
    didSendReady = NO;

    // set appearance to vibrantDark on < Mojave
    if (!IS_MOJAVE_AVAILABLE) {
        [self setAppearance:[NSAppearance appearanceNamed:NSAppearanceNameVibrantDark]];
    }

    [self setAnimationBehavior:NSWindowAnimationBehaviorDocumentWindow];
    [self setAcceptsMouseMovedEvents:YES];
    [self setDelegate:self];

    CAMetalLayer *layer = [CAMetalLayer layer];
    layer.pixelFormat = MTLPixelFormatRGBA16Float;
    layer.colorspace = CGColorSpaceCreateWithName(kCGColorSpaceACESCGLinear);
    layer.framebufferOnly = YES;
    layer.edgeAntialiasingMask = 0;
    layer.presentsWithTransaction = NO;
    layer.wantsExtendedDynamicRangeContent = YES;
    layer.contentsScale = self.backingScaleFactor;
    metalLayer = layer;
    [self.contentView setLayer:layer];

    CVDisplayLinkCreateWithActiveCGDisplays(&displayLink);
    CVDisplayLinkSetOutputCallback(displayLink, displayLinkCallback, (__bridge void *) self);

    [self makeKeyAndOrderFront:nil];
    shouldSendReadyOnUpdate = YES;

    return self;
}

- (void)doCallbackWithMainThread:(BOOL)isOnMainThread shouldRender:(BOOL)shouldRender {
    if (callback != nil) {
        callback(self, isOnMainThread, shouldRender);
    }
}

- (void)requestFrame {
    syncTimeout = 12;

    if (!displayLinkRunning) {
        CVDisplayLinkStart(displayLink);
        displayLinkRunning = YES;
    }
}

- (void)handleFrame {
    [self doCallbackWithMainThread:NO shouldRender:YES];

    if (syncTimeout > 0) {
        syncTimeout -= 1;
    } else {
        CVDisplayLinkStop(displayLink);
        displayLinkRunning = NO;
    }
}

- (void)pushNSEvent:(NSEvent*)event {
    if (!didSendReady) return;
    NCWindowEvent *windowEvent = [[NCWindowEvent alloc] init];
    windowEvent.eventType = NCWindowEventTypeNSEvent;
    windowEvent.event = event;
    [events addObject:windowEvent];
    [self doCallbackWithMainThread:YES shouldRender:NO];
    [self requestFrame];
}

- (void)pushWindowEvent:(NCWindowEventType)eventType {
    if (!didSendReady) return;
    NCWindowEvent *windowEvent = [[NCWindowEvent alloc] init];
    windowEvent.eventType = eventType;
    [events addObject:windowEvent];
    [self doCallbackWithMainThread:YES shouldRender:YES];
}

- (void)sendEvent:(NSEvent*)event {
    [super sendEvent:event];
    [self pushNSEvent:event];
}

- (NCWindowEvent *)dequeueEvent {
    if ([events count] == 0) {
        return nil;
    }
    NCWindowEvent *event = [events firstObject];
    [events removeObjectAtIndex:0];
    return event;
}

- (void)setDevice:(id<MTLDevice>)device {
    metalLayer.device = device;
}

- (NSColorSpace *)layerColorSpace {
    return [[NSColorSpace alloc] initWithCGColorSpace:metalLayer.colorspace];
}

- (void)windowDidResize:(NSNotification *)notification {
    [self pushWindowEvent:NCWindowEventTypeResized];
}

- (void)windowWillClose:(NSNotification *)notification {
    [self pushWindowEvent:NCWindowEventTypeWillClose];
}

- (void)windowDidUpdate:(NSNotification *)notification {
    if (shouldSendReadyOnUpdate) {
        didSendReady = YES;
        [self pushWindowEvent:NCWindowEventTypeReady];
        shouldSendReadyOnUpdate = NO;
    }
}

- (void)dealloc {
    if (displayLinkRunning) {
        CVDisplayLinkStop(displayLink);
        displayLinkRunning = NO;
    }
}

- (void)windowDidChangeBackingProperties:(NSNotification *)notification {
    metalLayer.contentsScale = self.backingScaleFactor;
    metalLayer.colorspace = self.colorSpace.CGColorSpace;
    [self pushWindowEvent:NCWindowEventTypeBackingUpdate];
}

- (BOOL)validateMenuItem:(NSMenuItem *)menuItem {
    return NO;
}

@end
