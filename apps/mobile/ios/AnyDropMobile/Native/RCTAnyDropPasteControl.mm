#import "RCTAnyDropPasteControl.h"
#import <React/RCTConversions.h>
#import <react/renderer/components/AnyDropSpec/ComponentDescriptors.h>
#import <react/renderer/components/AnyDropSpec/EventEmitters.h>
#import <react/renderer/components/AnyDropSpec/Props.h>

using namespace facebook::react;

@implementation RCTAnyDropPasteControl {
  UIPasteControl *_pasteControl;
}

+ (ComponentDescriptorProvider)componentDescriptorProvider {
  return concreteComponentDescriptorProvider<AnyDropPasteControlComponentDescriptor>();
}

- (instancetype)initWithFrame:(CGRect)frame {
  if (self = [super initWithFrame:frame]) {
    _props = std::make_shared<const AnyDropPasteControlProps>();
    self.pasteConfiguration = [[UIPasteConfiguration alloc] initWithAcceptableTypeIdentifiers:@[@"public.utf8-plain-text"]];
    UIPasteControlConfiguration *configuration = [UIPasteControlConfiguration new];
    configuration.displayMode = UIPasteControlDisplayModeIconAndLabel;
    configuration.cornerStyle = UIButtonConfigurationCornerStyleMedium;
    _pasteControl = [[UIPasteControl alloc] initWithConfiguration:configuration];
    _pasteControl.target = self;
    self.contentView = _pasteControl;
  }
  return self;
}

- (void)updateProps:(Props::Shared const &)props oldProps:(Props::Shared const &)oldProps {
  const auto &previous = static_cast<const AnyDropPasteControlProps &>(*_props);
  const auto &next = static_cast<const AnyDropPasteControlProps &>(*props);
  if (previous.tint != next.tint || previous.foreground != next.foreground) {
    UIPasteControlConfiguration *configuration = [UIPasteControlConfiguration new];
    configuration.displayMode = UIPasteControlDisplayModeIconAndLabel;
    configuration.cornerStyle = UIButtonConfigurationCornerStyleMedium;
    configuration.baseBackgroundColor = RCTUIColorFromSharedColor(next.tint);
    configuration.baseForegroundColor = RCTUIColorFromSharedColor(next.foreground);
    _pasteControl = [[UIPasteControl alloc] initWithConfiguration:configuration];
    _pasteControl.target = self;
    self.contentView = _pasteControl;
  }
  [super updateProps:props oldProps:oldProps];
}

- (void)pasteItemProviders:(NSArray<NSItemProvider *> *)itemProviders {
  // UIPasteControl grants this explicit paste; never inspect the general pasteboard.
  for (NSItemProvider *provider in itemProviders) {
    if (![provider canLoadObjectOfClass:NSString.class]) continue;
    auto emitter = std::static_pointer_cast<const AnyDropPasteControlEventEmitter>(_eventEmitter);
    [provider loadObjectOfClass:NSString.class completionHandler:^(id<NSItemProviderReading> object, NSError *error) {
      if (error || ![object isKindOfClass:NSString.class] || !emitter) return;
      NSString *text = (NSString *)object;
      NSData *utf8 = [text dataUsingEncoding:NSUTF8StringEncoding];
      if (!utf8) return;
      AnyDropPasteControlEventEmitter::OnPaste event = { .text = std::string(text.UTF8String ?: "", utf8.length) };
      emitter->onPaste(event);
    }];
    break;
  }
}
@end
