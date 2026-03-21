pragma Singleton
import QtQuick
import Quickshell.Services

Singleton {
    id: notifications
    
    property var lastNotification: null
    property bool hasNotification: false
    
    Connections {
        target: Quickshell.Notifications
        function onNotificationAdded(notification) {
            lastNotification = notification;
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
