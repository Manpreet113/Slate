pragma Singleton
import QtQuick

QtObject {
    // 1. Core Palette (Warm Ember)
    readonly property color bg: "#0A0A0A"
    readonly property real bgOpacity: 0.45
    readonly property color accent: "#FFB74D"
    readonly property color accentDim: Qt.rgba(255, 183, 77, 0.15)
    readonly property color borderColor: Qt.rgba(255, 255, 255, 0.08)
    readonly property color textPrimary: "#FFFFFF"
    readonly property color textSecondary: Qt.rgba(255, 255, 255, 0.6)

    // 2. Geometry
    readonly property real barHeight: 44
    readonly property real dockHeight: 65
    readonly property real radius: 16
    readonly property real padding: 10
    readonly property real margin: 12

    // 3. Effects (Molecular Glass)
    readonly property real blurRadius: 1.0 // MultiEffect scale
    readonly property real shadowOpacity: 0.15
    readonly property real shadowBlur: 0.8

    // 4. Typography
    readonly property string mainFont: "Inter"
    readonly property string monoFont: "JetBrains Mono"
    
    // 5. Motion (Fluidity)
    readonly property int durationFast: 200
    readonly property int durationSlow: 400
    readonly property var easing: Easing.OutQuint
}
