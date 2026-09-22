#import "RCTNativeAnyDrop.h"
#import "AnyDropMobile-Swift.h"
@implementation RCTNativeAnyDrop {
  AnyDropRuntime *_runtime;
}
+ (NSString *)moduleName { return @"NativeAnyDrop"; }
+ (BOOL)requiresMainQueueSetup { return YES; }
- (instancetype)init { if (self = [super init]) { _runtime = [AnyDropRuntime new]; } return self; }
- (std::shared_ptr<facebook::react::TurboModule>)getTurboModule:(const facebook::react::ObjCTurboModule::InitParams &)params { return std::make_shared<facebook::react::NativeAnyDropSpecJSI>(params); }
- (void)getSnapshot:(RCTPromiseResolveBlock)resolve reject:(RCTPromiseRejectBlock)reject { dispatch_async(dispatch_get_main_queue(), ^{ resolve([self->_runtime snapshot]); }); }
- (void)configure:(NSString *)settings resolve:(RCTPromiseResolveBlock)resolve reject:(RCTPromiseRejectBlock)reject { dispatch_async(dispatch_get_main_queue(), ^{ NSError *error = nil; NSString *state = [self->_runtime configure:settings error:&error]; if (error) reject(@"SETTINGS", error.localizedDescription, error); else resolve(state); }); }
- (void)start:(RCTPromiseResolveBlock)resolve reject:(RCTPromiseRejectBlock)reject { dispatch_async(dispatch_get_main_queue(), ^{ NSError *error = nil; NSString *state = [self->_runtime startAndReturnError:&error]; if (error) reject(@"START", error.localizedDescription, error); else resolve(state); }); }
- (void)stop:(RCTPromiseResolveBlock)resolve reject:(RCTPromiseRejectBlock)reject { dispatch_async(dispatch_get_main_queue(), ^{ resolve([self->_runtime stop]); }); }
- (void)sendText:(NSString *)text resolve:(RCTPromiseResolveBlock)resolve reject:(RCTPromiseRejectBlock)reject { dispatch_async(dispatch_get_main_queue(), ^{ [self->_runtime sendText:text completion:^(NSString *state, NSError *error) { if (error) reject(@"SEND", error.localizedDescription, error); else resolve(state); }]; }); }
- (void)getClipboard:(RCTPromiseResolveBlock)resolve reject:(RCTPromiseRejectBlock)reject { dispatch_async(dispatch_get_main_queue(), ^{ NSError *error = nil; NSString *text = [self->_runtime clipboardAndReturnError:&error]; if (error) reject(@"PASTE", error.localizedDescription, error); else resolve(text); }); }
- (void)copyText:(NSString *)text resolve:(RCTPromiseResolveBlock)resolve reject:(RCTPromiseRejectBlock)reject { dispatch_async(dispatch_get_main_queue(), ^{ [self->_runtime copyText:text]; resolve(nil); }); }
@end
