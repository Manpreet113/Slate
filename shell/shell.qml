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
        
        anchors {
            bottom: true
            left: true
            right: true
        }
        
        property bool revealed: false
        
        // Dynamic Height: Trigger area (5px) vs Full Area
        implicitHeight: revealed ? (Config.dockHeight + Config.margin) : 5
        
        // No exclusive zone so it doesn't push windows
        exclusiveZone: -1 
        color: "transparent"
        
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "slate-dock"
        
        // This MouseArea now only exists when implicitHeight > 5 if we want, 
        // but keeping it simple: it's the trigger.
        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            onEntered: bottomDock.revealed = true
            onExited: bottomDock.revealed = false
        }

        Dock {
            id: dockContent
            width: 600
            anchors.horizontalCenter: parent.horizontalCenter
            
            // Slide animation
            anchors.bottom: parent.bottom
            anchors.bottomMargin: bottomDock.revealed ? Config.margin : -Config.dockHeight
            
            opacity: bottomDock.revealed ? 1.0 : 0.0
            
            Behavior on anchors.bottomMargin {
                NumberAnimation { duration: 250; easing.type: Easing.OutCubic }
            }
            Behavior on opacity {
                NumberAnimation { duration: 200 }
            }
        }
    }
}
