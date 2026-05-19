using Microsoft.Windows.AppNotifications;
using Microsoft.Windows.AppNotifications.Builder;
using System;
using System.Diagnostics;
using Windows.UI.Notifications;

namespace SRCounter;

public class NotificationUtils
{
    private static bool _didManagerInitialized = false;

    public static void ShowNotification(string content)
    {
        var manager = AppNotificationManager.Default;

        if (!_didManagerInitialized)
        {
            AppNotificationManager.Default.NotificationInvoked += (AppNotificationManager sender, AppNotificationActivatedEventArgs args) =>
            {
            };
            manager.Register();
            _didManagerInitialized = true;
        }

        Debug.WriteLine($"Notification: {content}");

        manager.Show(
            new AppNotificationBuilder()
                .AddText(content)
                .SetDuration(AppNotificationDuration.Long)
                .AddButton(new AppNotificationButton("OK").AddArgument("action", "OK"))
            .BuildNotification()
        );
    }
}
