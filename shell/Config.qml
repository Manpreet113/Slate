pragma Singleton
import QtQuick
import Quickshell

Singleton {
    // Colors
    readonly property color bg: "#1A1A1A"
    readonly property color fg: "#FFFFFF"
    readonly property color fgDim: "#AAAAAA"
    readonly property color accent: "#FF9500" // Warm Amber
    readonly property color accentDim: "#CC7700"
    readonly property real bgOpacity: 0.8
    
    // Gradients
    readonly property list<color> glassGradient: [Qt.rgba(1, 1, 1, 0.1), Qt.rgba(1, 1, 1, 0.02)]
    
    // Glassmorphism
    readonly property real blurRadius: 24
    readonly property real borderOpacity: 0.15
    readonly property color borderColor: Qt.rgba(1, 1, 1, borderOpacity)
    
    // Geometry
    readonly property real barHeight: 44
    readonly property real dockHeight: 60
    readonly property real radius: 22 // Pill style
    readonly property real margin: 16
    readonly property real padding: 10
    
    // Fonts
    readonly property string sansFont: "Inter"
    readonly property string monoFont: "JetBrains Mono"
    readonly property real fontSize: 13
}
