@import Cocoa;

#define IS_YOSEMITE_AVAILABLE (NSAppKitVersionNumber >= NSAppKitVersionNumber10_10)
#define IS_EL_CAPITAN_AVAILABLE (NSAppKitVersionNumber >= NSAppKitVersionNumber10_11)

// there’s no NSAppKitVersionNumber10_14 constant so here’s `NSAppKitVersion.current` from
// a Swift Playground run on Mojave
#define IS_MOJAVE_AVAILABLE (NSAppKitVersionNumber >= 1671.0)
