import QtQuick
import Quickshell
import Quickshell.Wayland
import "."

ShellRoot {
    id: root
    
    // Global settings
    settings.watchFiles: true
    
    // The top bar
    PanelWindow {
        id: topBar
        
        anchors {
            top: true
            left: true
            right: true
        }
        
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
    
    // The bottom dock
    PanelWindow {
        id: bottomDock
        
        anchors {
            bottom: true
            left: true
            right: true
        }
        
        implicitHeight: Config.dockHeight + Config.margin
        color: "transparent"
        
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "slate-dock"
        
        Dock {
            width: 600
            height: Config.dockHeight
            anchors.bottom: parent.bottom
            anchors.bottomMargin: Config.margin
            anchors.horizontalCenter: parent.horizontalCenter
        }
    }
}
