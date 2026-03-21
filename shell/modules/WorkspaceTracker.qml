import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Hyprland
import ".."

RowLayout {
    id: workspaceTracker
    spacing: Config.padding
    
    Repeater {
        model: Hyprland.workspaces
        
        delegate: Rectangle {
            readonly property bool isFocused: Hyprland.focusedWorkspace === modelData
            
            width: isFocused ? 24 : 8
            height: 8
            radius: 4
            color: isFocused ? Config.accent : Config.fg
            opacity: isFocused ? 1.0 : 0.4
            
            Behavior on width { NumberAnimation { duration: 250; easing.type: Easing.OutQuint } }
            Behavior on opacity { NumberAnimation { duration: 250 } }
            
            MouseArea {
                anchors.fill: parent
                onClicked: Hyprland.dispatch("workspace " + modelData.id)
            }
        }
    }
}
