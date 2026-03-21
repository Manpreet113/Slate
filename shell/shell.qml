//@ pragma Env QSG_RENDER_LOOP=threaded
import QtQuick
import Quickshell
import Quickshell.Wayland
import "modules" as Modules
import "."

ShellRoot {
    id: root
    
    // UI State
    property bool showCommandCenter: false
    property bool showLauncher: false
    property bool showDashboard: false
    
    // Global settings
    settings.watchFiles: true
    
    // The top bar
    PanelWindow {
        id: topBar
        anchors { top: true; left: true; right: true }
        implicitHeight: Config.barHeight + Config.margin
        exclusiveZone: Config.barHeight + Config.margin
        color: "transparent"
        WlrLayershell.layer: WlrLayer.Top
        WlrLayershell.namespace: "slate-bar"
        
        Bar {
            anchors.fill: parent
            anchors.margins: Config.margin
            anchors.bottomMargin: 0
        }
    }
    
    // The Notch
    PanelWindow {
        id: notchWindow
        anchors {
            top: true
            horizontalCenter: true
        }
        WlrLayershell.margins.top: 4
        
        color: "transparent"
        WlrLayershell.layer: WlrLayer.Top
        WlrLayershell.namespace: "slate-notch"
        
        Modules.Notch {
            anchors.fill: parent
        }
    }

    // Notification Popups
    PanelWindow {
        id: notificationWindow
        anchors {
            top: true
            right: true
        }
        WlrLayershell.margins.top: Config.barHeight + Config.margin * 2
        WlrLayershell.margins.right: Config.margin
        
        color: "transparent"
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "slate-notifications"
        
        Modules.NotificationPopup {
            anchors.fill: parent
        }
    }

    // The Dashboard Overlay
    PanelWindow {
        id: dashboardWindow
        visible: root.showDashboard
        
        anchors {
            top: true
            bottom: true
            left: true
            right: true
        }
        
        color: "transparent"
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "slate-dashboard"
        
        MouseArea {
            anchors.fill: parent
            onClicked: root.showDashboard = false
        }
        
        Modules.Dashboard {
            width: 400
            height: 600
            anchors.centerIn: parent
            
            MouseArea {
                anchors.fill: parent
                onClicked: {}
            }
        }
    }
}
