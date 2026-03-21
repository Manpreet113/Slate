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
    
    // The bottom dock
    PanelWindow {
        id: bottomDock
        
        anchors { bottom: true; left: true; right: true }
        implicitHeight: Config.dockHeight + Config.margin
        
        // Autohide: No exclusive zone so it overlays windows
        exclusiveZone: -1 
        color: "transparent"
        
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "slate-dock"
        
        property bool autohide: true
        property bool revealed: false
        
        // Hit area for autohide
        MouseArea {
            id: hitArea
            anchors.fill: parent
            hoverEnabled: true
            onEntered: bottomDock.revealed = true
        }

        Dock {
            id: dockContent
            width: 600
            anchors.horizontalCenter: parent.horizontalCenter
            
            // Animation logic
            anchors.bottom: parent.bottom
            anchors.bottomMargin: (bottomDock.revealed || !bottomDock.autohide) ? Config.margin : -Config.dockHeight
            
            Behavior on anchors.bottomMargin {
                NumberAnimation { duration: 300; easing.type: Easing.OutQuart }
            }
            
            // Hide after mouse leaves the dock itself
            MouseArea {
                anchors.fill: parent
                hoverEnabled: true
                propagateComposedEvents: true
                onExited: bottomDock.revealed = false
            }
        }
    }
}
