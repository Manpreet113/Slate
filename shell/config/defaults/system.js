.pragma library

var data = {
    "disks": ["/"],
    "updateServiceEnabled": true,
    "idle": {
        "general": {
            "lock_cmd": "slate lock",
            "before_sleep_cmd": "loginctl lock-session",
            "after_sleep_cmd": "slate screen on"
        },
        "listeners": [
            {
                "timeout": 150,
                "onTimeout": "slate brightness 10 -s",
                "onResume": "slate brightness -r"
            },
            {
                "timeout": 300,
                "onTimeout": "loginctl lock-session"
            },
            {
                "timeout": 330,
                "onTimeout": "slate screen off",
                "onResume": "slate screen on"
            },
            {
                "timeout": 1800,
                "onTimeout": "slate suspend"
            }
        ]
    },
    "ocr": {
        "eng": true,
        "spa": true,
        "lat": false,
        "jpn": false,
        "chi_sim": false,
        "chi_tra": false,
        "kor": false
    },
    "pomodoro": {
        "workTime": 1500,
        "restTime": 300,
        "autoStart": false,
        "syncSpotify": false
    }
}
