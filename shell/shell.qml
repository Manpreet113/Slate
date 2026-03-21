//@ pragma Env QSG_RENDER_LOOP=threaded
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
            bottom: true
            left: true
            right: true
        }
        
        color: "transparent"
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "slate-command-center"
        
        // Full screen click-to-close area
        MouseArea {
            anchors.fill: parent
            onClicked: root.showCommandCenter = false
        }
        
        CommandCenter {
            width: 350
            height: 500
            anchors.top: parent.top
            anchors.right: parent.right
            anchors.topMargin: Config.barHeight + Config.margin * 2
            anchors.rightMargin: Config.margin
            
            // Re-intercept clicks so they don't close the window
            MouseArea {
                anchors.fill: parent
                onClicked: {} 
            }
        }
    }
}
