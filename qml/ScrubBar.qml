import QtQuick

// The seek bar, split into the video's chapters, with SponsorBlock's skipped parts in green.
// Click or drag to seek; hover shows the time.
Item {
    id: bar

    property real duration: 0
    property real position: 0
    property real buffered: 0           // seconds ahead of position
    property var chapters: []           // [{start_time, title}]
    property var skips: []              // [{start, end, label}]

    signal seek(real seconds)

    readonly property bool dragging: mouse.pressed
    readonly property real shown: dragging ? dragPos : position
    property real dragPos: 0

    implicitHeight: 18

    readonly property var segments: {
        const d = duration > 0 ? duration : 1
        const starts = chapters.length > 1 ? chapters.map(c => c.start_time) : [0]
        return starts.map((s, i) => ({ start: s, end: i + 1 < starts.length ? starts[i + 1] : d }))
    }

    function timeAt(x) { return Math.max(0, Math.min(1, x / width)) * duration }

    Row {
        anchors.verticalCenter: parent.verticalCenter
        width: parent.width
        spacing: 3
        Repeater {
            model: bar.segments
            delegate: Rectangle {
                required property var modelData
                readonly property real span: modelData.end - modelData.start
                width: Math.max(2, (bar.width - 3 * (bar.segments.length - 1)) * span / Math.max(1, bar.duration))
                height: mouse.containsMouse || bar.dragging ? 7 : 5
                radius: 3
                color: Qt.rgba(1, 1, 1, 0.22)
                clip: true
                Behavior on height { NumberAnimation { duration: 90 } }
                Rectangle {
                    height: parent.height
                    color: Qt.rgba(1, 1, 1, 0.28)
                    width: parent.width * Math.max(0, Math.min(1, (bar.position + bar.buffered - parent.modelData.start) / parent.span))
                }
                Rectangle {
                    height: parent.height
                    color: Theme.accent
                    width: parent.width * Math.max(0, Math.min(1, (bar.shown - parent.modelData.start) / parent.span))
                }
            }
        }
    }

    // Skipped parts (approximate across the gaps between chapters, which are a few pixels).
    Repeater {
        model: bar.duration > 0 ? bar.skips : []
        delegate: Rectangle {
            required property var modelData
            x: bar.width * modelData.start / bar.duration
            width: Math.max(2, bar.width * (modelData.end - modelData.start) / bar.duration)
            height: mouse.containsMouse || bar.dragging ? 7 : 5
            anchors.verticalCenter: parent.verticalCenter
            radius: 2
            color: Qt.rgba(0.36, 0.85, 0.45, 0.75)
        }
    }

    Rectangle {
        width: 14; height: 14; radius: 7
        color: "white"
        x: (bar.duration > 0 ? bar.shown / bar.duration : 0) * bar.width - 7
        anchors.verticalCenter: parent.verticalCenter
        border.width: 4
        border.color: Qt.rgba(1, 1, 1, 0.18)
        visible: mouse.containsMouse || bar.dragging
    }

    // Hover label: time and chapter.
    Rectangle {
        visible: mouse.containsMouse && bar.duration > 0
        readonly property real t: bar.timeAt(mouse.mouseX)
        readonly property string chapter: {
            let title = ""
            for (const c of bar.chapters) if (c.start_time <= t) title = c.title
            return title
        }
        x: Math.max(0, Math.min(bar.width - width, mouse.mouseX - width / 2))
        y: -height - 10
        width: tip.implicitWidth + 16
        height: 26
        radius: 6
        color: "#23262c"
        Text {
            id: tip
            anchors.centerIn: parent
            text: Theme.timecode(parent.t) + (parent.chapter !== "" ? "  ·  " + parent.chapter : "")
            color: Theme.text
            font.family: Theme.font
            font.pixelSize: 12
        }
    }

    MouseArea {
        id: mouse
        anchors.fill: parent
        anchors.topMargin: -8
        anchors.bottomMargin: -8
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onPressed: (e) => bar.dragPos = bar.timeAt(e.x)
        onPositionChanged: (e) => { if (pressed) bar.dragPos = bar.timeAt(e.x) }
        onReleased: bar.seek(bar.dragPos)
    }
}
