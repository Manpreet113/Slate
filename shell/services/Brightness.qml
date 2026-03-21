pragma Singleton
import QtQuick
import Quickshell.Io

QtObject {
    id: brightness
    
    property int value: 70
    
    function setValue(val) {
        let v = Math.min(100, Math.max(0, val));
        value = v;
        setProc.run(["brightnessctl", "s", v + "%"]);
    }
    
    Process {
        id: getProc
        command: ["brightnessctl", "g"]
        running: true
        stdout: SplitParser {
            onRead: (data) => {
                let val = parseInt(data.trim());
                if (!isNaN(val)) brightness.value = Math.round((val / 255) * 100);
            }
        }
    }
    
    Process { id: setProc }
    
    Timer {
        interval: 5000
        running: true
        repeat: true
        onTriggered: getProc.run()
    }
}
