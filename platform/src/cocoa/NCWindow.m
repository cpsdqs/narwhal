#import "NCWindow.h"
#import "available.h"

@implementation NCWindowEvent
@synthesize eventType;
@synthesize event;

- (NSString *)description
{
    return [NSString stringWithFormat:@"<%@: %p> %@", [self class], self, self.event];
}
@end

@implementation NCWindow

@synthesize deviceType;
@synthesize metalLayer;
@synthesize openGLContext;
@synthesize callbackData;

- (instancetype)initWithContentRect:(NSRect)contentRect
                           callback:(void (*)(NCWindow*))callbackFn
                             device:(NCWindowDevice)device {
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

    // set appearance to vibrantDark on Yosemite <= x < Mojave
    if (IS_YOSEMITE_AVAILABLE && !IS_MOJAVE_AVAILABLE) {
        [self setAppearance:[NSAppearance appearanceNamed:NSAppearanceNameVibrantDark]];
    }

    [self setAnimationBehavior:NSWindowAnimationBehaviorDocumentWindow];
    [self setAcceptsMouseMovedEvents:YES];
    [self setDelegate:self];

    if (IS_EL_CAPITAN_AVAILABLE && device == NCWindowDeviceMetal) {
        deviceType = NCWindowDeviceMetal;
        CAMetalLayer *layer = [[CAMetalLayer alloc] init];
        layer.pixelFormat = MTLPixelFormatRGBA16Float;
        // layer.colorspace = self.colorSpace.CGColorSpace;
        layer.colorspace = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
        layer.framebufferOnly = YES;
        layer.edgeAntialiasingMask = 0;
        layer.presentsWithTransaction = NO;
        layer.wantsExtendedDynamicRangeContent = YES;
        layer.contentsScale = self.backingScaleFactor;
        metalLayer = layer;
        [self.contentView setLayer:layer];
    } else if (device == NCWindowDeviceOpenGL) {
        deviceType = NCWindowDeviceOpenGL;
        NSOpenGLPixelFormatAttribute attrs[] = {
            NSOpenGLPFADoubleBuffer,
            NSOpenGLPFAClosestPolicy,
            NSOpenGLPFAColorSize, 32,
            NSOpenGLPFAAlphaSize, 8,
            NSOpenGLPFADepthSize, 24,
            NSOpenGLPFAStencilSize, 8,
            NSOpenGLPFAAllowOfflineRenderers,
            NSOpenGLPFAOpenGLProfile, NSOpenGLProfileVersion3_2Core,
            NSOpenGLPFAMultisample,
            NSOpenGLPFASampleBuffers, 1,
            NSOpenGLPFASamples, 4,
            0
        };
        NSOpenGLPixelFormat *pixelFormat = [[NSOpenGLPixelFormat alloc] initWithAttributes:attrs];
        NSOpenGLView *openGLView = [[NSOpenGLView alloc] initWithFrame:NSMakeRect(0., 0., 0., 0.)
                                                           pixelFormat:pixelFormat];
        [openGLView setWantsBestResolutionOpenGLSurface:YES];
        [self setContentView:openGLView];
        openGLContext = [openGLView openGLContext];
        [openGLContext makeCurrentContext];

        // don’t really know what this does; copied it from somewhere
        CGLEnable([openGLContext CGLContextObj], kCGLCECrashOnRemovedFunctions);
    } else {
        [self release];
        return nil;
    }

    [self makeKeyAndOrderFront:nil];
    shouldSendReadyOnUpdate = YES;

    return self;
}

- (void)_doCallback {
    if (callback != nil) {
        callback(self);
    }
}

- (void)pushNSEvent:(NSEvent*)event {
    if (!didSendReady) return;
    NCWindowEvent *windowEvent = [[NCWindowEvent alloc] init];
    windowEvent.eventType = NCWindowEventTypeNSEvent;
    windowEvent.event = event;
    [events addObject:windowEvent];
    [self _doCallback];
}

- (void)pushWindowEvent:(NCWindowEventType)eventType {
    if (!didSendReady) return;
    NCWindowEvent *windowEvent = [[NCWindowEvent alloc] init];
    windowEvent.eventType = eventType;
    [events addObject:windowEvent];
    [self _doCallback];
}

- (void)sendEvent:(NSEvent*)event {
    [super sendEvent:event];
    [self pushNSEvent:event];
}

- (NSArray *)drainEvents {
    NSArray* result = events;
    events = [[NSMutableArray alloc] initWithCapacity:2];
    return result;
}

- (void)setDevice:(id<MTLDevice>)device {
    if (IS_EL_CAPITAN_AVAILABLE) {
        ((CAMetalLayer *) self.metalLayer).device = device;
    }
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

- (void)windowDidChangeBackingProperties:(NSNotification *)notification {
    if (IS_EL_CAPITAN_AVAILABLE) {
        ((CAMetalLayer *) self.metalLayer).contentsScale = self.backingScaleFactor;
        ((CAMetalLayer *) self.metalLayer).colorspace = self.colorSpace.CGColorSpace;
        [self pushWindowEvent:NCWindowEventTypeBackingUpdate];
    }
}

- (BOOL)validateMenuItem:(NSMenuItem *)menuItem {
    return NO;
}

@end
