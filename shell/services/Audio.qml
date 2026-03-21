pragma Singleton
import QtQuick
import Quickshell.Io

Item {
    id: audio
    
    property int volume: 50
    property bool muted: false
    
    // Setters
    function setVolume(val) {
        let v = Math.min(100, Math.max(0, val));
        volume = v;
        setProc.run(["wpctl", "set-volume", "@DEFAULT_AUDIO_SINK@", v + "%"]);
    }
    
    function toggleMute() {
        muted = !muted;
        setProc.run(["wpctl", "set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"]);
    }
    
    // Getters / Updaters
    Process {
        id: getProc
        command: ["wpctl", "get-volume", "@DEFAULT_AUDIO_SINK@"]
        running: true
        stdout: SplitParser {
            onRead: (data) => {
                // Parse "Volume: 0.50 [MUTED]"
                let match = data.match(/Volume: ([\d.]+)(?: \[MUTED\])?/);
                if (match) {
                    audio.volume = Math.round(parseFloat(match[1]) * 100);
                    audio.muted = data.includes("[MUTED]");
                }
            }
        }
    }
    
    Process { id: setProc }
    
    Timer {
        interval: 3000
        running: true
        repeat: true
        onTriggered: getProc.run()
    }
}
