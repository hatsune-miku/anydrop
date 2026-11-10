#import <Cocoa/Cocoa.h>
#import <FinderSync/FinderSync.h>

@interface AnyDropFinderSync : FIFinderSync
@property(copy) NSArray<NSURL *> *menuURLs;
@end

@implementation AnyDropFinderSync
- (instancetype)init {
    self = [super init];
    if (self) {
        // Includes the user's folders, external drives and mounted network volumes.
        FIFinderSyncController.defaultController.directoryURLs =
            [NSSet setWithObject:[NSURL fileURLWithPath:@"/" isDirectory:YES]];
    }
    return self;
}

- (NSMenu *)menuForMenuKind:(FIMenuKind)kind {
    if (kind != FIMenuKindContextualMenuForItems && kind != FIMenuKindContextualMenuForContainer) {
        return nil;
    }
    FIFinderSyncController *controller = FIFinderSyncController.defaultController;
    NSArray<NSURL *> *urls = kind == FIMenuKindContextualMenuForItems ? controller.selectedItemURLs : nil;
    if (urls.count == 0 && controller.targetedURL) urls = @[controller.targetedURL];
    if (urls.count == 0) return nil;
    // Finder proxies menu items across processes. Keep the captured selection
    // in the extension, rather than relying on NSMenuItem.representedObject.
    self.menuURLs = urls;

    NSMenu *menu = [[NSMenu alloc] initWithTitle:@""];
    for (NSString *title in @[@"使用 AnyDrop 发送", @"复制绝对路径"]) {
        SEL action = [title isEqualToString:@"复制绝对路径"] ? @selector(copyPaths:) : @selector(sendFiles:);
        NSMenuItem *item = [[NSMenuItem alloc] initWithTitle:title action:action keyEquivalent:@""];
        item.target = self;
        [menu addItem:item];
    }
    return menu;
}

- (void)copyPaths:(NSMenuItem *)sender {
    NSArray<NSURL *> *urls = self.menuURLs;
    if (urls.count == 0) return;
    NSString *paths = [[urls valueForKey:@"path"] componentsJoinedByString:@"\n"];
    [NSPasteboard.generalPasteboard clearContents];
    [NSPasteboard.generalPasteboard setString:paths forType:NSPasteboardTypeString];
}

- (void)sendFiles:(NSMenuItem *)sender {
    if (self.menuURLs.count == 0) return;
    NSURLComponents *request = [[NSURLComponents alloc] init];
    request.scheme = @"anydrop";
    request.host = @"send-files";
    NSMutableArray<NSURLQueryItem *> *items = [NSMutableArray array];
    for (NSURL *url in self.menuURLs) {
        [items addObject:[NSURLQueryItem queryItemWithName:@"path" value:url.path]];
    }
    request.queryItems = items;
    // URL form encoding decodes '+' as a space in the receiver.
    request.percentEncodedQuery = [request.percentEncodedQuery stringByReplacingOccurrencesOfString:@"+" withString:@"%2B"];
    // Target the containing app explicitly, even if another build owns the URL scheme.
    NSURL *app = NSBundle.mainBundle.bundleURL;
    for (int i = 0; i < 3; i++) app = app.URLByDeletingLastPathComponent;
    if (@available(macOS 10.15, *)) {
        NSWorkspaceOpenConfiguration *configuration = [NSWorkspaceOpenConfiguration configuration];
        configuration.allowsRunningApplicationSubstitution = NO;
        [NSWorkspace.sharedWorkspace openURLs:@[request.URL] withApplicationAtURL:app
                               configuration:configuration
                           completionHandler:^(NSRunningApplication *application, NSError *error) {
            if (error) NSLog(@"AnyDrop could not open file selection: %@", error);
        }];
    } else {
        NSError *error = nil;
        [NSWorkspace.sharedWorkspace openURLs:@[request.URL] withApplicationAtURL:app
                                     options:NSWorkspaceLaunchDefault configuration:@{} error:&error];
        if (error) NSLog(@"AnyDrop could not open file selection: %@", error);
    }
}
@end
