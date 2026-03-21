pragma Singleton
import QtQuick
import Quickshell.Services.Notifications

Item {
    id: notifications
    
    property var lastNotification: null
    property bool hasNotification: false
    
    NotificationServer {
        onNotification: (n) => {
            lastNotification = n;
            hasNotification = true;
            hideTimer.restart();
        }
    }
    
    Timer {
        id: hideTimer
        interval: 5000
        onTriggered: hasNotification = false
    }
}
