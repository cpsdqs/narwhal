@import Cocoa;
@import Metal;
@import QuartzCore;

NS_ASSUME_NONNULL_BEGIN

/*!
 @enum NCWindowDevice
 @abstract Possible rendering devices for windows.
 */
typedef enum: uint32 {
    /*!
     OpenGL 3.2 core
     */
    NCWindowDeviceOpenGL = 0,
    /*!
     Metal 1.0 probably
     */
    NCWindowDeviceMetal = 1,
} NCWindowDevice;

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

/**
 A window with narwhal-rendered content.
 */
@interface NCWindow : NSWindow <NSWindowDelegate> {
    NSMutableArray *events;
    void (*callback)(NCWindow *);
    BOOL didSendReady;
    BOOL shouldSendReadyOnUpdate;
}

/**
 The window’s rendering device type.
 */
@property (readonly, nonatomic) NCWindowDevice deviceType;

/**
 The window’s CAMetalLayer (type is id for backward compatibility).
 Will only be set if the Metal backend is used.
 */
@property (readonly, nullable, nonatomic) id metalLayer;

/**
 The window’s OpenGLContext.
 Will only be set if the OpenGL backend is used.
 */
@property (readonly, nullable, nonatomic, retain) NSOpenGLContext *openGLContext;

/**
 Callback data for Rust.
 */
@property (nonatomic) NCWindowCallbackData callbackData;

/**
 Initializes the window with a contentRect (see NSWindow methods) and a callback function.

 @param contentRect the content rectangle on the screen.
 @return the window.
 */
- (instancetype)initWithContentRect:(NSRect)contentRect
                           callback:(void (*)(NCWindow*))callbackFn
                             device:(NCWindowDevice)device;

- (NSArray *)drainEvents;

- (void)setDevice:(id<MTLDevice>)device;

@end

NS_ASSUME_NONNULL_END
