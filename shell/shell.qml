import QtQuick
import Quickshell
import Quickshell.Wayland
import "."

ShellRoot {
    id: root
    
    // UI State
    property bool showCommandCenter: false
    property bool showLauncher: false
    
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
    
    // The Command Center Overlay
    PanelWindow {
        id: commandCenterWindow
        visible: root.showCommandCenter
        
        anchors {
            top: true
            right: true
        }
        
        WlrLayershell.margins.top: Config.barHeight + Config.margin * 2
        WlrLayershell.margins.right: Config.margin
        
        implicitWidth: 350
        implicitHeight: 500
        color: "transparent"
        
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "slate-command-center"
        
        CommandCenter {
            anchors.fill: parent
        }
    }
}
