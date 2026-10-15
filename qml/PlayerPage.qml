import QtCore
import QtQuick
import QtQuick.Layouts
import QtQuick.Effects
import QtQuick.Dialogs
import QtQuick.Templates as T
import org.kde.kirigami as Kirigami
import dev.subfeed

// The player (direction A): video with fading controls, title and actions below, and a side
// panel with chapters, description and what's next in the feed.
//   Space/K pause   ←/→ or J/L 10 s   ↑/↓ volume   M mute   [ ] speed   , . chapter
//   F fullscreen    I panel           W watched     0 start over        Esc back
//   C captions (off by default; the choice is remembered)
//   N save notes (the transcript as Markdown, e.g. into an Obsidian vault); Shift+N picks the folder
// SponsorBlock: sponsor reads, self-promotion and subscribe reminders are skipped (Undo plays one).
FocusScope {
    id: page

    required property var mpv           // MpvItem (owned by Main, lent to this page)
    required property var session       // PlayerSession
    required property var feed          // FeedModel
    required property var video         // the feed row that was opened
    property bool fullscreen: false
    property bool panelOpen: true

    signal back()
    signal fullscreenToggled()
    signal playOther(var video)

    readonly property string videoId: video.videoId
    readonly property var chapters: JSON.parse(session.chaptersJson)
    readonly property int chapterIndex: {
        let i = -1
        for (let n = 0; n < chapters.length; ++n) if (chapters[n].start_time <= mpv.position + 0.5) i = n
        return i
    }
    readonly property var upNext: JSON.parse(feed.upNext(videoId, 3))
    readonly property var skips: JSON.parse(session.skipSegmentsJson)
    // Segments the user chose to watch after all (Undo), by index.
    property var allowedSkips: ({})
    property bool markedWatched: false
    property bool finished: false
    property bool controlsPoked: true
    property bool firstLoad: true
    property double openedAt: 0
    readonly property bool controlsShown: controlsPoked || mpv.paused || !mpv.loaded || scrub.dragging || finished
    readonly property string displayTitle: session.title !== "" ? session.title : video.title
    // The video's shape: from mpv once it plays, guessed before that. Anything narrower than
    // the 16:9 frame (a vertical Short, an old 4:3 video) plays in a centred column with a
    // blurred backdrop beside it instead of black bars.
    readonly property real videoAspect: mpv.aspect > 0 ? mpv.aspect : video.isShort ? 9 / 16 : 16 / 9
    readonly property bool narrow: videoAspect < 16 / 9 - 0.05

    focus: true
    Component.onCompleted: {
        mpv.parent = mpvHolder
        mpv.anchors.fill = mpvHolder
        mpv.opacity = 1
        openVideo()
        poke()
    }

    function leave() {
        if (mpv.loaded) session.savePosition(mpv.position)
        mpv.stop()
        session.close()
        back()
    }
    function poke() { controlsPoked = true; hideTimer.restart() }
    function openVideo() {
        firstLoad = true
        allowedSkips = ({})
        openedAt = Date.now()
        session.open(videoId, SystemInfo.onBattery, SystemInfo.powerSaver, SystemInfo.decodableCodecs.join(","),
                     Qt.locale().name)
    }
    function seekChapter(step) {
        if (chapters.length === 0) return
        const target = step < 0 && chapterIndex >= 0 && mpv.position - chapters[chapterIndex].start_time > 3
            ? chapterIndex : chapterIndex + step
        if (target >= 0 && target < chapters.length) mpv.seekTo(chapters[target].start_time)
    }
    function markWatched(watched) {
        session.setWatched(watched)
        markedWatched = watched
    }
    function copyLink() {
        clip.text = "https://www.youtube.com/watch?v=" + videoId
        clip.selectAll()
        clip.copy()
        toast.show("Link copied")
    }

    // Notes: a folder is asked for the first time (and with Shift+N), then remembered.
    Settings {
        id: notesPrefs
        category: "notes"
        property string folder: ""
    }
    function saveNotes(chooseFolder) {
        if (session.state !== "ready") {
            toast.show("Notes can be saved once the video has loaded")
        } else if (chooseFolder || notesPrefs.folder === "") {
            folderDialog.open()
        } else {
            session.saveNotes(notesPrefs.folder)
        }
    }
    FolderDialog {
        id: folderDialog
        title: "Where should notes go?"
        currentFolder: notesPrefs.folder !== "" ? "file://" + notesPrefs.folder
                       : StandardPaths.writableLocation(StandardPaths.DocumentsLocation)
        onAccepted: {
            notesPrefs.folder = decodeURIComponent(selectedFolder.toString().replace(/^file:\/\//, ""))
            page.session.saveNotes(notesPrefs.folder)
        }
    }
    Connections {
        target: page.session
        function onNotesSaved(path, openUrl, existed) {
            const folder = notesPrefs.folder.split("/").pop()
            toast.show(existed ? "Notes for this video are already in “" + folder + "”" : "Saved notes to “" + folder + "”",
                       "Open", () => Qt.openUrlExternally(openUrl))
        }
        function onNotesFailed(reason) { toast.show(reason) }
    }

    Timer { id: hideTimer; interval: 2500; onTriggered: page.controlsPoked = false }
    Timer {
        interval: 5000; repeat: true
        running: page.mpv.loaded && !page.mpv.paused
        onTriggered: page.session.savePosition(page.mpv.position)
    }
    TextEdit { id: clip; visible: false }

    Connections {
        target: page.session
        function onVideoUrlChanged() {
            if (page.session.videoUrl === "") return
            if (page.firstLoad) console.log("subfeed: stream chosen " + ((Date.now() - page.openedAt) / 1000).toFixed(1) + " s after opening")
            page.finished = false
            const wasPaused = !page.firstLoad && page.mpv.paused
            page.mpv.loadStream(page.session.videoUrl, page.session.audioUrl, page.session.httpHeaders,
                                page.session.resumeFrom, page.session.subtitleUrl)
            if (wasPaused) page.mpv.paused = true
            if (page.firstLoad) {
                if (page.session.resumeFrom > 0) resumeChip.show()
            } else if (page.session.switchNote !== "") {
                toast.show(page.session.switchNote)
            }
            page.firstLoad = false
        }
    }
    Connections {
        target: page.mpv
        function onLoadedChanged() {
            if (page.mpv.loaded && page.openedAt > 0) {
                console.log("subfeed: playing " + ((Date.now() - page.openedAt) / 1000).toFixed(1) + " s after opening")
                page.openedAt = 0
            }
        }
        function onPositionChanged() {
            if (!page.markedWatched && page.mpv.duration > 60 && page.mpv.position >= page.mpv.duration * 0.95) page.markWatched(true)
            const t = page.mpv.position
            for (let i = 0; i < page.skips.length; i++) {
                const s = page.skips[i]
                if (t >= s.start && t < s.end - 0.5 && !page.allowedSkips[i]) {
                    page.mpv.seekTo(s.end)
                    toast.show("Skipped " + s.label, "Undo", () => {
                        page.allowedSkips[i] = true
                        page.mpv.seekTo(s.start)
                    })
                    break
                }
            }
        }
        function onPausedChanged() { if (page.mpv.loaded) page.session.savePosition(page.mpv.position) }
        function onFinished() {
            if (!page.markedWatched) page.markWatched(true)
            page.session.savePosition(0)
            page.finished = true
        }
        function onLoadFailed(reason) { toast.show(reason) }
    }

    Settings {
        id: prefs
        category: "player"
        property bool captionsOn: false
    }
    Binding { target: page.mpv; property: "subtitlesVisible"; value: prefs.captionsOn }
    // Captions step up above the seek bar and buttons while those show.
    Binding {
        target: page.mpv
        property: "subtitlePosition"
        value: page.controlsShown && mpvHolder.height > 0 ? Math.round(100 * (1 - 110 / mpvHolder.height)) : 100
    }
    function toggleCaptions() {
        if (session.subtitleUrl === "") {
            toast.show("No captions for this video")
            return
        }
        prefs.captionsOn = !prefs.captionsOn
        toast.show(prefs.captionsOn ? "Captions on · " + session.subtitleLabel : "Captions off")
    }

    Keys.onPressed: (event) => {
        page.poke()
        switch (event.key) {
        case Qt.Key_Space:
        case Qt.Key_K: mpv.paused = !mpv.paused; break
        case Qt.Key_Left:
        case Qt.Key_J: mpv.seekBy(-10); break
        case Qt.Key_Right:
        case Qt.Key_L: mpv.seekBy(10); break
        case Qt.Key_Up: mpv.volume = mpv.volume + 5; break
        case Qt.Key_Down: mpv.volume = mpv.volume - 5; break
        case Qt.Key_M: mpv.muted = !mpv.muted; break
        case Qt.Key_BracketLeft: mpv.speed = Math.round((mpv.speed - 0.25) * 4) / 4; break
        case Qt.Key_BracketRight: mpv.speed = Math.round((mpv.speed + 0.25) * 4) / 4; break
        case Qt.Key_Equal: mpv.speed = 1; break
        case Qt.Key_Comma: seekChapter(-1); break
        case Qt.Key_Period: seekChapter(1); break
        case Qt.Key_0: mpv.seekTo(0); resumeChip.visible = false; break
        case Qt.Key_F: fullscreenToggled(); break
        case Qt.Key_I: panelOpen = !panelOpen; break
        case Qt.Key_C: toggleCaptions(); break
        case Qt.Key_N: saveNotes(event.modifiers & Qt.ShiftModifier); break
        case Qt.Key_W: markWatched(!markedWatched); toast.show(markedWatched ? "Marked watched" : "Marked unwatched"); break
        case Qt.Key_R: if (session.state === "error") openVideo(); break
        case Qt.Key_Return:
        case Qt.Key_Enter:
            if (finished && upNext.length > 0) playOther(upNext[0])
            break
        case Qt.Key_Escape:
            if (fullscreen) fullscreenToggled()
            else leave()
            break
        default: return
        }
        event.accepted = true
    }

    Rectangle { anchors.fill: parent; color: Theme.bg }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        Item {
            id: left
            Layout.fillWidth: true
            Layout.fillHeight: true

            Rectangle {
                id: videoBox
                readonly property real infoHeight: 190
                width: page.fullscreen ? left.width : Math.min(left.width, (left.height - infoHeight) * 16 / 9)
                height: page.fullscreen ? left.height : width * 9 / 16
                anchors.horizontalCenter: parent.horizontalCenter
                color: "black"
                clip: true

                // Beside a narrow video: its thumbnail, blurred and dimmed. Only built for narrow
                // videos, so a normal one doesn't pay for the blur.
                Loader {
                    anchors.fill: parent
                    active: page.narrow && (page.video.hero || page.video.thumbnail || "") !== ""
                    sourceComponent: Item {
                        clip: true
                        Image {
                            id: backdropImage
                            // A Short's hqdefault is 4:3 with its vertical frame in the middle;
                            // size it so that frame alone covers the box.
                            readonly property real frameHeight: Math.max(parent.width * 16 / 9, parent.height)
                            anchors.centerIn: parent
                            width: page.video.isShort ? frameHeight * 4 / 3 : parent.width
                            height: page.video.isShort ? frameHeight : parent.height
                            source: page.video.hero || page.video.thumbnail
                            fillMode: Image.PreserveAspectCrop
                            sourceSize.width: 480
                            visible: false
                        }
                        MultiEffect {
                            anchors.fill: backdropImage
                            source: backdropImage
                            blurEnabled: true; blur: 1.0; blurMax: 64
                            brightness: -0.3
                            saturation: 0.2
                        }
                    }
                }

                Item {
                    id: mpvHolder
                    anchors.centerIn: parent
                    width: page.narrow ? Math.min(parent.width, parent.height * page.videoAspect) : parent.width
                    height: parent.height
                }

                MouseArea {
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: page.controlsShown ? Qt.ArrowCursor : Qt.BlankCursor
                    onPositionChanged: page.poke()
                    onClicked: { page.mpv.paused = !page.mpv.paused; page.poke() }
                    onDoubleClicked: page.fullscreenToggled()
                }

                // Loading: the thumbnail, blurred, with what's happening.
                Item {
                    anchors.fill: parent
                    visible: page.session.state === "loading" || (page.session.state === "ready" && !page.mpv.loaded)
                    // A narrow video's backdrop already shows behind this, so no frame here. (The
                    // effect stays visible either way: a hidden MultiEffect stalled QML timers.)
                    Image {
                        id: loadingFrame
                        anchors.fill: parent
                        source: page.narrow ? "" : page.video.hero || page.video.thumbnail || ""
                        fillMode: Image.PreserveAspectCrop
                        visible: false
                    }
                    MultiEffect {
                        anchors.fill: parent
                        source: loadingFrame
                        blurEnabled: true; blur: 1.0; blurMax: 48
                        brightness: -0.55
                    }
                    Column {
                        anchors.centerIn: parent
                        spacing: 14
                        width: parent.width - 80
                        Kirigami.Icon {
                            anchors.horizontalCenter: parent.horizontalCenter
                            source: Theme.icon("loader-circle")
                            isMask: true
                            color: Theme.accent
                            width: 32; height: 32
                            RotationAnimator on rotation { from: 0; to: 360; duration: 1100; loops: Animation.Infinite; running: parent ? parent.visible : false }
                        }
                        Text {
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            text: page.displayTitle
                            color: Theme.text
                            font.family: Theme.font
                            font.pixelSize: 17
                            font.weight: Font.DemiBold
                            elide: Text.ElideRight
                        }
                        Text {
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            text: page.session.quality === "" ? "Finding a stream…"
                                : (page.session.onBattery ? "Saving power · " : "") + page.session.quality
                            color: "#b9bec6"
                            font.family: Theme.font
                            font.pixelSize: 13
                        }
                    }
                }

                // Error.
                Rectangle {
                    anchors.fill: parent
                    visible: page.session.state === "error"
                    color: "#15171b"
                    Column {
                        anchors.centerIn: parent
                        width: Math.min(parent.width - 112, 520)
                        spacing: 12
                        Kirigami.Icon {
                            anchors.horizontalCenter: parent.horizontalCenter
                            source: Theme.icon("triangle-alert")
                            isMask: true
                            color: Theme.warning
                            width: 34; height: 34
                        }
                        Text {
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.Wrap
                            text: page.session.errorTitle
                            color: Theme.text
                            font.family: Theme.font
                            font.pixelSize: 17
                            font.weight: Font.DemiBold
                        }
                        Text {
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.Wrap
                            maximumLineCount: 4
                            elide: Text.ElideRight
                            text: page.session.errorDetail
                            color: "#b9bec6"
                            font.family: Theme.font
                            font.pixelSize: 13
                            lineHeight: 1.2
                        }
                        Row {
                            anchors.horizontalCenter: parent.horizontalCenter
                            topPadding: 6
                            spacing: 8
                            SfButton { primary: true; text: "Retry"; hint: "R"; size: 40; onClicked: page.openVideo() }
                            SfButton { text: "Copy link"; size: 40; tone: Theme.hover; onClicked: page.copyLink() }
                        }
                    }
                }

                // Finished: no autoplay; offer the next video.
                Rectangle {
                    anchors.fill: parent
                    visible: page.finished
                    color: Qt.rgba(0, 0, 0, 0.72)
                    Column {
                        anchors.centerIn: parent
                        spacing: 16
                        Row {
                            anchors.horizontalCenter: parent.horizontalCenter
                            spacing: 8
                            Kirigami.Icon { source: Theme.icon("check"); isMask: true; color: Theme.accent; width: 18; height: 18 }
                            Text { text: "Marked watched"; color: Theme.text; font.family: Theme.font; font.pixelSize: 15; font.weight: Font.DemiBold }
                        }
                        Rectangle {
                            visible: page.upNext.length > 0
                            anchors.horizontalCenter: parent.horizontalCenter
                            width: nextRow.implicitWidth + 24
                            height: 83
                            radius: Theme.radiusM
                            color: Qt.rgba(0.09, 0.1, 0.11, 0.92)
                            Row {
                                id: nextRow
                                anchors.verticalCenter: parent.verticalCenter
                                x: 10
                                spacing: 12
                                Kirigami.ShadowedImage {
                                    width: 112; height: 63; radius: 8
                                    source: page.upNext.length > 0 ? page.upNext[0].thumbnail : ""
                                    fillMode: Image.PreserveAspectCrop
                                }
                                Column {
                                    anchors.verticalCenter: parent.verticalCenter
                                    spacing: 4
                                    Text { text: "NEXT IN YOUR FEED"; color: "#8a919b"; font.family: Theme.font; font.pixelSize: 11; font.weight: Font.DemiBold; font.letterSpacing: 1.3 }
                                    Text {
                                        text: page.upNext.length > 0 ? page.upNext[0].title : ""
                                        color: Theme.text; font.family: Theme.font; font.pixelSize: 14; font.weight: Font.DemiBold
                                        width: Math.min(implicitWidth, 420); elide: Text.ElideRight
                                    }
                                    Text {
                                        text: page.upNext.length > 0 ? Theme.meta([page.upNext[0].channelTitle, page.upNext[0].duration]) + "  ·  Enter play  ·  Esc feed" : ""
                                        color: Theme.text2; font.family: Theme.font; font.pixelSize: 12
                                    }
                                }
                            }
                            TapHandler { onTapped: page.playOther(page.upNext[0]) }
                        }
                    }
                }

                // Resumed chip.
                Rectangle {
                    id: resumeChip
                    function show() { visible = true; resumeTimer.restart() }
                    visible: false
                    anchors { left: parent.left; bottom: controls.top; leftMargin: 20; bottomMargin: 4 }
                    width: resumeRow.implicitWidth + 24
                    height: 40
                    radius: 10
                    color: Qt.rgba(0.08, 0.08, 0.09, 0.85)
                    Timer { id: resumeTimer; interval: 6000; onTriggered: resumeChip.visible = false }
                    Row {
                        id: resumeRow
                        anchors.centerIn: parent
                        spacing: 14
                        Text { text: "Resumed from " + Theme.timecode(page.session.resumeFrom); color: Theme.text; font.family: Theme.font; font.pixelSize: 14 }
                        Text { text: "Start over"; color: Theme.accent; font.family: Theme.font; font.pixelSize: 14; font.weight: Font.Medium
                            TapHandler { onTapped: { page.mpv.seekTo(0); resumeChip.visible = false } } }
                        Kbd { text: "0"; anchors.verticalCenter: parent.verticalCenter }
                    }
                }

                // Top: back.
                Rectangle {
                    anchors { left: parent.left; right: parent.right; top: parent.top }
                    height: 90
                    opacity: page.controlsShown ? 1 : 0
                    Behavior on opacity { NumberAnimation { duration: 200 } }
                    gradient: Gradient {
                        GradientStop { position: 0; color: Qt.rgba(0, 0, 0, 0.7) }
                        GradientStop { position: 1; color: "transparent" }
                    }
                    SfButton {
                        x: 20; y: 16
                        size: 36
                        tone: Qt.rgba(0.08, 0.08, 0.1, 0.6)
                        iconName: "chevron-left"
                        text: "Feed"
                        hint: "Esc"
                        onClicked: page.fullscreen ? page.fullscreenToggled() : page.leave()
                    }
                    Text {
                        visible: page.fullscreen
                        anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: 140; rightMargin: 40; topMargin: 24 }
                        text: page.displayTitle
                        color: Theme.text
                        font.family: Theme.font
                        font.pixelSize: 16
                        font.weight: Font.DemiBold
                        elide: Text.ElideRight
                    }
                }

                // Bottom: chapter, seek bar, controls.
                Rectangle {
                    id: controls
                    anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
                    height: 140
                    opacity: page.controlsShown ? 1 : 0
                    visible: opacity > 0
                    Behavior on opacity { NumberAnimation { duration: 200 } }
                    gradient: Gradient {
                        GradientStop { position: 0; color: "transparent" }
                        GradientStop { position: 1; color: Qt.rgba(0, 0, 0, 0.85) }
                    }

                    Text {
                        visible: page.chapterIndex >= 0
                        anchors { left: parent.left; bottom: scrub.top; leftMargin: 20; bottomMargin: 10 }
                        textFormat: Text.StyledText
                        text: "<font color='" + Theme.accent + "'><b>Chapter " + (page.chapterIndex + 1) + "</b></font>&nbsp;&nbsp;"
                              + (page.chapterIndex >= 0 ? page.chapters[page.chapterIndex].title.replace(/&/g, "&amp;").replace(/</g, "&lt;") : "")
                        color: "#d5d8dd"
                        font.family: Theme.font
                        font.pixelSize: 13
                    }

                    ScrubBar {
                        id: scrub
                        anchors { left: parent.left; right: parent.right; bottom: controlRow.top; leftMargin: 20; rightMargin: 20; bottomMargin: 8 }
                        duration: page.mpv.duration > 0 ? page.mpv.duration : page.session.duration
                        position: page.mpv.position
                        buffered: page.mpv.bufferedAhead
                        chapters: page.chapters
                        skips: page.skips
                        onSeek: (t) => { page.mpv.seekTo(t); page.poke() }
                    }

                    RowLayout {
                        id: controlRow
                        anchors { left: parent.left; right: parent.right; bottom: parent.bottom; leftMargin: 20; rightMargin: 20; bottomMargin: 12 }
                        spacing: 6

                        SfButton {
                            primary: true
                            iconName: page.mpv.paused ? "play" : "pause"
                            onClicked: page.mpv.paused = !page.mpv.paused
                        }
                        IconTool { iconName: "rotate-ccw"; tip: "Back 10 s (←)"; onClicked: page.mpv.seekBy(-10) }
                        IconTool { iconName: "rotate-cw"; tip: "Forward 10 s (→)"; onClicked: page.mpv.seekBy(10) }
                        IconTool {
                            iconName: page.mpv.muted || page.mpv.volume === 0 ? "volume-x" : "volume-2"
                            tip: "Mute (M)"
                            onClicked: page.mpv.muted = !page.mpv.muted
                        }
                        Rectangle {
                            Layout.preferredWidth: 70
                            Layout.preferredHeight: 4
                            radius: 2
                            color: Qt.rgba(1, 1, 1, 0.25)
                            Rectangle { width: parent.width * Math.min(1, page.mpv.volume / 100); height: 4; radius: 2; color: page.mpv.muted ? Theme.text3 : Theme.text }
                            MouseArea {
                                anchors.fill: parent; anchors.margins: -8
                                onPressed: (e) => page.mpv.volume = Math.round(Math.max(0, Math.min(1, (e.x - 8) / 70)) * 100)
                                onPositionChanged: (e) => { if (pressed) page.mpv.volume = Math.round(Math.max(0, Math.min(1, (e.x - 8) / 70)) * 100) }
                            }
                        }
                        Text {
                            Layout.leftMargin: 12
                            textFormat: Text.StyledText
                            text: Theme.timecode(scrub.shown) + " <font color='#8a919b'>/ " + Theme.timecode(scrub.duration) + "</font>"
                            color: Theme.text
                            font.family: Theme.mono
                            font.pixelSize: 13
                        }
                        Item { Layout.fillWidth: true }
                        IconTool {
                            iconName: "captions"
                            tip: page.session.subtitleUrl === "" ? "No captions" : "Captions (C)"
                            on: prefs.captionsOn && page.session.subtitleUrl !== ""
                            opacity: page.session.subtitleUrl === "" ? 0.35 : 1
                            onClicked: page.toggleCaptions()
                        }
                        SfButton {
                            size: 32
                            tone: Qt.rgba(1, 1, 1, 0.1)
                            text: page.mpv.speed.toFixed(2).replace(/\.?0+$/, "") + "×"
                            font.family: Theme.mono
                            font.pixelSize: 12
                            onClicked: page.mpv.speed = page.mpv.speed >= 2 ? 1 : page.mpv.speed + 0.25
                        }
                        Rectangle {
                            visible: page.session.quality !== ""
                            Layout.preferredHeight: 32
                            Layout.preferredWidth: badge.implicitWidth + 20
                            radius: 8
                            color: "transparent"
                            border.color: Qt.rgba(1, 1, 1, 0.18)
                            Row {
                                id: badge
                                anchors.centerIn: parent
                                spacing: 6
                                Kirigami.Icon {
                                    visible: page.session.onBattery
                                    source: Theme.icon("battery-medium")
                                    isMask: true
                                    color: "#c8ccd2"
                                    width: 14; height: 14
                                    anchors.verticalCenter: parent.verticalCenter
                                }
                                Text {
                                    text: page.session.quality + " · " + (page.mpv.hwdec === "" || page.mpv.hwdec === "no" ? "CPU" : "GPU")
                                    color: "#c8ccd2"
                                    font.family: Theme.font
                                    font.pixelSize: 12
                                }
                            }
                        }
                        IconTool { iconName: page.panelOpen ? "panel-right-close" : "panel-right-open"; tip: "Panel (I)"; visible: !page.fullscreen; onClicked: page.panelOpen = !page.panelOpen }
                        IconTool {
                            iconName: page.fullscreen ? "minimize" : "maximize"
                            tip: "Fullscreen (F)"
                            onClicked: page.fullscreenToggled()
                        }
                    }
                }
            }

            // Below the video: title, actions, keys.
            ColumnLayout {
                visible: !page.fullscreen
                anchors { top: videoBox.bottom; left: parent.left; right: parent.right; leftMargin: 32; rightMargin: 32; topMargin: 22 }
                spacing: 10

                Text {
                    Layout.fillWidth: true
                    text: page.displayTitle
                    color: Theme.text
                    font.family: Theme.font
                    font.pixelSize: 26
                    font.weight: Font.Bold
                    font.letterSpacing: -0.5
                    elide: Text.ElideRight
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 10
                    Text {
                        Layout.fillWidth: true
                        text: [page.session.channel || page.video.channelTitle, page.session.published,
                               page.session.duration > 0 ? Theme.timecode(page.session.duration) : ""].filter(s => s).join(" · ")
                        color: Theme.text2
                        font.family: Theme.font
                        font.pixelSize: 14
                        elide: Text.ElideRight
                    }
                    SfButton {
                        size: 40
                        iconName: "check"
                        text: page.markedWatched ? "Watched" : "Mark watched"
                        hint: "W"
                        tone: page.markedWatched ? Theme.hover : Theme.raised
                        onClicked: page.markWatched(!page.markedWatched)
                    }
                    SfButton {
                        size: 40
                        iconName: "file-text"
                        text: page.session.savingNotes ? "Saving…" : "Save notes"
                        hint: "N"
                        enabled: !page.session.savingNotes
                        onClicked: page.saveNotes(false)
                    }
                    SfButton { size: 40; iconName: "copy"; text: "Copy link"; onClicked: page.copyLink() }
                }
                Rectangle {
                    Layout.fillWidth: true
                    Layout.topMargin: 10
                    implicitHeight: hints.implicitHeight + 28
                    radius: Theme.radiusM
                    color: "#15171b"
                    Flow {
                        id: hints
                        anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; leftMargin: 16; rightMargin: 16 }
                        spacing: 24
                        Repeater {
                            model: [["Space", "pause"], ["← →", "10 s"], ["[ ]", "speed"], [", .", "chapter"], ["C", "captions"], ["N", "notes"], ["F", "fullscreen"], ["I", "panel"], ["Esc", "feed"]]
                            delegate: Row {
                                required property var modelData
                                spacing: 6
                                Text { text: parent.modelData[0]; color: "#d5d8dd"; font.family: Theme.mono; font.pixelSize: 12 }
                                Text { text: parent.modelData[1]; color: Theme.text2; font.family: Theme.font; font.pixelSize: 13 }
                            }
                        }
                    }
                }
            }
        }

        // Side panel: chapters / description, then next in the feed.
        Rectangle {
            id: sidePanel
            visible: page.panelOpen && !page.fullscreen
            Layout.preferredWidth: 360
            Layout.fillHeight: true
            color: Theme.panel
            Rectangle { width: 1; height: parent.height; color: Theme.hairline }

            property int tab: page.chapters.length > 0 ? 0 : 1

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 22
                anchors.topMargin: 18
                spacing: 14

                Row {
                    spacing: 6
                    Repeater {
                        model: ["Chapters", "Description"]
                        delegate: T.Button {
                            required property string modelData
                            required property int index
                            readonly property bool on: sidePanel.tab === index
                            width: tabLabel.implicitWidth + 24
                            height: 32
                            focusPolicy: Qt.NoFocus
                            onClicked: sidePanel.tab = index
                            background: Rectangle { radius: 8; color: parent.on ? "#24272d" : "transparent" }
                            contentItem: Text {
                                id: tabLabel
                                text: parent.modelData
                                color: parent.on ? Theme.text : "#8a919b"
                                font.family: Theme.font
                                font.pixelSize: 13
                                font.weight: Font.Medium
                                horizontalAlignment: Text.AlignHCenter
                                verticalAlignment: Text.AlignVCenter
                            }
                        }
                    }
                }

                Flickable {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    contentHeight: panelBody.height
                    boundsBehavior: Flickable.StopAtBounds

                    Column {
                        id: panelBody
                        width: parent.width
                        spacing: 2

                        Text {
                            visible: sidePanel.tab === 0 && page.chapters.length === 0
                            text: page.session.state === "loading" ? "Loading chapters…" : "This video has no chapters."
                            color: Theme.text3
                            font.family: Theme.font
                            font.pixelSize: 13
                        }
                        Repeater {
                            model: sidePanel.tab === 0 ? page.chapters : []
                            delegate: Rectangle {
                                id: chapterRow
                                required property var modelData
                                required property int index
                                readonly property bool current: index === page.chapterIndex
                                width: panelBody.width
                                height: chapterText.implicitHeight + 20
                                radius: 10
                                color: current ? "#1d2026" : chapterHover.hovered ? "#181a1f" : "transparent"
                                Text {
                                    x: 12; y: 10
                                    width: 44
                                    text: Theme.timecode(chapterRow.modelData.start_time)
                                    color: chapterRow.current ? Theme.accent : Theme.text3
                                    font.family: Theme.mono
                                    font.pixelSize: 12
                                }
                                Text {
                                    id: chapterText
                                    x: 60; y: 10
                                    width: parent.width - 72
                                    text: chapterRow.modelData.title
                                    wrapMode: Text.Wrap
                                    color: chapterRow.current ? Theme.text : chapterRow.index < page.chapterIndex ? "#7d848e" : "#c8ccd2"
                                    font.family: Theme.font
                                    font.pixelSize: 14
                                    font.weight: chapterRow.current ? Font.DemiBold : Font.Normal
                                }
                                HoverHandler { id: chapterHover; cursorShape: Qt.PointingHandCursor }
                                TapHandler { onTapped: page.mpv.seekTo(chapterRow.modelData.start_time) }
                            }
                        }
                        Text {
                            visible: sidePanel.tab === 1
                            width: parent.width
                            wrapMode: Text.Wrap
                            textFormat: Text.StyledText
                            linkColor: Theme.accent
                            text: {
                                const d = page.session.description
                                if (d === "") return page.session.state === "loading" ? "Loading…" : "No description."
                                const esc = d.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
                                return esc.replace(/(https?:\/\/[^\s<]+)/g, "<a href=\"$1\">$1</a>").replace(/\n/g, "<br>")
                            }
                            color: Theme.text2
                            font.family: Theme.font
                            font.pixelSize: 13
                            lineHeight: 1.3
                            onLinkActivated: (link) => Qt.openUrlExternally(link)
                            HoverHandler { cursorShape: parent.hoveredLink !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor }
                        }
                    }
                }

                Text {
                    visible: page.upNext.length > 0
                    text: "NEXT IN YOUR FEED"
                    color: Theme.text3
                    font.family: Theme.font
                    font.pixelSize: 12
                    font.weight: Font.DemiBold
                    font.letterSpacing: 1.4
                }
                Repeater {
                    model: page.upNext
                    delegate: RowLayout {
                        id: nextItem
                        required property var modelData
                        Layout.fillWidth: true
                        spacing: 12
                        Kirigami.ShadowedImage {
                            // A Short: its vertical frame, cropped out of the 4:3 thumbnail.
                            Layout.preferredWidth: nextItem.modelData.isShort ? 33 : 104
                            Layout.preferredHeight: 58
                            radius: 8
                            color: Theme.surface
                            source: nextItem.modelData.thumbnail
                            fillMode: Image.PreserveAspectCrop
                            sourceSize.width: 208
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 3
                            Text {
                                Layout.fillWidth: true
                                text: nextItem.modelData.title
                                color: Theme.text
                                font.family: Theme.font
                                font.pixelSize: 13
                                font.weight: Font.DemiBold
                                wrapMode: Text.Wrap
                                maximumLineCount: 2
                                elide: Text.ElideRight
                            }
                            Text {
                                Layout.fillWidth: true
                                text: Theme.meta([nextItem.modelData.channelTitle, nextItem.modelData.duration])
                                color: "#8a919b"
                                font.family: Theme.font
                                font.pixelSize: 12
                                elide: Text.ElideRight
                            }
                        }
                        HoverHandler { cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: page.playOther(nextItem.modelData) }
                    }
                }
            }
        }
    }

    Toast { id: toast }

    component IconTool: T.Button {
        property string iconName
        property string tip
        // Toggled on: the icon takes the accent.
        property bool on: false
        implicitWidth: 40
        implicitHeight: 40
        focusPolicy: Qt.NoFocus
        hoverEnabled: true
        background: Rectangle { radius: 10; color: parent.hovered ? Qt.rgba(1, 1, 1, 0.1) : "transparent" }
        contentItem: Item {
            Kirigami.Icon {
                anchors.centerIn: parent
                width: 20; height: 20
                source: Theme.icon(parent.parent.iconName)
                isMask: true
                color: parent.parent.on ? Theme.accent : Theme.text
            }
        }
    }
}
