import QtQuick
import QtQuick.Layouts
import ".."

Rectangle {
    id: root
    property string icon: ""
    property string label: ""
    property bool active: false
    signal clicked()
    
    Layout.fillWidth: true
    height: 60
    radius: Config.radiusMedium
    color: active ? Config.accent : "white"
    opacity: active ? 0.8 : 0.05
    
    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: 15; spacing: 10
        Text { text: root.icon; color: "white"; font.pixelSize: 20 }
        Text { text: root.label; color: "white"; font.pixelSize: 12; font.bold: true; Layout.fillWidth: true; elide: Text.ElideRight }
    }
    
    MouseArea { anchors.fill: parent; onClicked: root.clicked() }
}
