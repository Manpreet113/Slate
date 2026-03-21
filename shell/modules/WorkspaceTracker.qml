import QtQuick
import QtQuick.Layouts
import QtQuick.Effects
import Quickshell
import Quickshell.Hyprland
import ".."

Item {
    id: workspaceTracker
    height: 12
    width: indexLayout.width
    
    // The "Sliding Pill"
    Rectangle {
        id: activePill
        height: 12
        width: 12
        radius: 6
        color: Config.accent
        
        // Logic for sliding (Assume workspaces 1-6)
        readonly property int focusedId: Hyprland.focusedWorkspace ? Hyprland.focusedWorkspace.id : 1
        x: (focusedId - 1) * (8 + Config.padding) - 2
        anchors.verticalCenter: parent.verticalCenter
        
        Behavior on x {
            NumberAnimation { duration: Config.durationFast; easing.type: Config.easing }
        }
        
        layer.enabled: true
        layer.effect: MultiEffect {
            shadowEnabled: true
            shadowOpacity: 0.4
            shadowBlur: 0.6
            shadowColor: Config.accent
        }
    }

    Row {
        id: indexLayout
        spacing: Config.padding
        anchors.verticalCenter: parent.verticalCenter
        
        Repeater {
            model: 6
            delegate: Item {
                width: 8
                height: 8
                
                readonly property int workspaceId: index + 1
                readonly property bool isFocused: Hyprland.focusedWorkspace && Hyprland.focusedWorkspace.id === workspaceId
                readonly property bool isOccupied: {
                    if (!Hyprland.workspaces) return false;
                    for (var i = 0; i < Hyprland.workspaces.length; i++) {
                        let ws = Hyprland.workspaces.get(i);
                        if (ws && ws.id === workspaceId) return true;
                    }
                    return false;
                }
                
                Rectangle {
                    anchors.centerIn: parent
                    width: 6; height: 6; radius: 3
                    color: "white"
                    opacity: isFocused ? 0.0 : (isOccupied ? 0.6 : 0.2)
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
