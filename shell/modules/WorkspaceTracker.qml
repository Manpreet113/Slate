import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Hyprland
import ".."

RowLayout {
    id: workspaceTracker
    spacing: Config.padding
    
    Repeater {
        // Show 1-5 as dots, common in modern rices
        model: 5
        
        delegate: Rectangle {
            readonly property int workspaceId: index + 1
            readonly property bool isFocused: Hyprland.focusedWorkspace && Hyprland.focusedWorkspace.id === workspaceId
            readonly property bool isOccupied: {
                for (var i = 0; i < Hyprland.workspaces.length; i++) {
                    if (Hyprland.workspaces[i].id === workspaceId) return true;
                }
                return false;
            }
            
            width: isFocused ? 24 : 8
            height: 8
            radius: 4
            
            // Accent for focused, Dim white for occupied, Very dim for empty
            color: isFocused ? Config.accent : (isOccupied ? Config.fg : Config.fg)
            opacity: isFocused ? 1.0 : (isOccupied ? 0.6 : 0.2)
            
            Behavior on width { NumberAnimation { duration: 250; easing.type: Easing.OutQuint } }
            Behavior on opacity { NumberAnimation { duration: 250 } }
            
            MouseArea {
                anchors.fill: parent
                onClicked: Hyprland.dispatch("workspace " + workspaceId)
            }
        }
    }
}
