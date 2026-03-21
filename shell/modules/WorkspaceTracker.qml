import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Hyprland
import ".."

Item {
    id: workspaceTracker
    height: 12
    width: indexLayout.width
    
    // The "Sliding Pill" background for the active workspace
    Rectangle {
        id: activePill
        height: 6
        width: 20
        radius: 3
        color: Config.accent
        
        // Find the X position of the focused workspace dot
        x: {
            let focused = -1;
            for (let i = 1; i <= 5; i++) {
                if (Hyprland.focusedWorkspace && Hyprland.focusedWorkspace.id === i) {
                    focused = i - 1;
                    break;
                }
            }
            if (focused === -1) focused = 0; // Default to 1
            return focused * (8 + Config.padding) + (8 - width)/2;
        }
        
        anchors.verticalCenter: parent.verticalCenter
        
        Behavior on x {
            NumberAnimation { duration: 350; easing.type: Easing.OutQuint }
        }
    }

    Row {
        id: indexLayout
        spacing: Config.padding
        anchors.verticalCenter: parent.verticalCenter
        
        Repeater {
            model: 5
            delegate: Item {
                width: 8
                height: 8
                
                readonly property int workspaceId: index + 1
                readonly property bool isFocused: Hyprland.focusedWorkspace && Hyprland.focusedWorkspace.id === workspaceId
                readonly property bool isOccupied: {
                    for (var i = 0; i < Hyprland.workspaces.length; i++) {
                        if (Hyprland.workspaces[i].id === workspaceId) return true;
                    }
                    return false;
                }
                
                // The Dot
                Rectangle {
                    anchors.centerIn: parent
                    width: 6
                    height: 6
                    radius: 3
                    
                    // Invisible if focused (because the pill is there), otherwise white/dim
                    color: "white"
                    opacity: isFocused ? 0.0 : (isOccupied ? 0.7 : 0.2)
                    
                    Behavior on opacity { NumberAnimation { duration: 250 } }
                }
                
                MouseArea {
                    anchors.fill: parent
                    onClicked: Hyprland.dispatch("workspace " + workspaceId)
                }
            }
        }
    }
}
