import QtCore
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import QtQuick.Templates as T
import org.kde.kirigami as Kirigami

// The feed (direction A): header, channel pills, the newest video large, then the grid.
// The Shorts view swaps in a grid of tall cards (and no large video); Shorts never mix in.
//   j/k or arrows  move (up from the first row reaches the large video)
//   Enter  play    w  watched    h  hide (undo)    u  unwatched only    s  Shorts view
//   c  this channel    /  search    Esc  clear search / channel / Shorts    F5  refresh
//   m  manage channels
// Settling on a video (half a second of hover or keyboard focus) looks it up in the background,
// so opening it is quick.
FocusScope {
    id: page

    required property var feed          // FeedModel
    required property var channels      // ChannelModel
    property var ytdlp: null            // YtdlpStatus: a notice when yt-dlp should be updated
    property string channelFilter: ""
    property string channelFilterTitle: ""
    property bool unwatchedOnly: false
    property bool shortsMode: false
    property bool heroFocused: false
    readonly property string searchText: search.text.trim()
    readonly property var hero: feed.heroJson !== "" ? JSON.parse(feed.heroJson) : null
    readonly property var continuing: JSON.parse(feed.continueJson)

    signal playRequested(var video)
    signal manageRequested()
    signal noticeCopied()
    signal hidden(string videoId, string title, bool isHero)

    function applyFilter() {
        const keep = grid.currentItem ? grid.currentItem.videoId : ""
        feed.setFilter(channelFilter, searchText, unwatchedOnly, shortsMode)
        grid.currentIndex = keep !== "" ? Math.max(0, feed.rowOf(keep)) : 0
        if (grid.currentIndex === 0) Qt.callLater(grid.positionViewAtBeginning)
        if (!hero) heroFocused = false
    }
    onChannelFilterChanged: applyFilter()
    onUnwatchedOnlyChanged: applyFilter()
    onShortsModeChanged: {
        heroFocused = false
        applyFilter()
        grid.currentIndex = 0
        grid.positionViewAtBeginning()
        focusGrid()
    }

    function focusGrid() { grid.forceActiveFocus() }
    // For --tour: move the selection like j/k.
    function moveSelection(step) { grid.moveBy(step) }

    // Prefetch: the video the pointer or keyboard has settled on. Launching the app or scrolling
    // past doesn't count, so an idle feed costs no lookups.
    function settle(video, on) {
        if (!video) return
        if (on) {
            prefetchTimer.videoId = video.videoId
            prefetchTimer.restart()
        } else if (prefetchTimer.videoId === video.videoId) {
            prefetchTimer.stop()
        }
    }
    Timer {
        id: prefetchTimer
        property string videoId
        interval: 500
        onTriggered: page.feed.prefetch(videoId)
    }
    onPlayRequested: prefetchTimer.stop()
    function selectChannel(id, title) {
        channelFilterTitle = title
        channelFilter = id
        heroFocused = false
        grid.positionViewAtBeginning()
        focusGrid()
    }

    Rectangle { anchors.fill: parent; color: Theme.bg }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // Header: wordmark, search, All/Unwatched, refresh.
        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: 64
            Layout.leftMargin: Theme.gutter
            Layout.rightMargin: Theme.gutter
            spacing: 24

            Row {
                spacing: 10
                Rectangle { width: 10; height: 10; radius: 3; color: Theme.accent; anchors.verticalCenter: parent.verticalCenter }
                Text {
                    text: "subfeed"
                    color: Theme.text
                    font.family: Theme.font
                    font.pixelSize: 19
                    font.weight: Font.Bold
                    font.letterSpacing: -0.4
                }
            }

            T.TextField {
                id: search
                Layout.leftMargin: 40
                Layout.fillWidth: true
                Layout.maximumWidth: 460
                implicitHeight: 40
                leftPadding: 40
                rightPadding: 40
                color: Theme.text
                placeholderText: "Search your subscriptions"
                placeholderTextColor: Theme.placeholder
                font.family: Theme.font
                font.pixelSize: 14
                verticalAlignment: TextInput.AlignVCenter
                selectByMouse: true
                selectionColor: Theme.accent
                selectedTextColor: Theme.accentInk
                background: Rectangle {
                    radius: Theme.radiusM
                    color: Theme.surface
                    border.width: search.activeFocus ? 1 : 0
                    border.color: Theme.accent
                    Kirigami.Icon {
                        source: Theme.icon("search")
                        isMask: true
                        color: Theme.placeholder
                        width: 16; height: 16
                        anchors { left: parent.left; leftMargin: 14; verticalCenter: parent.verticalCenter }
                    }
                    Text {
                        visible: search.text === "" && !search.preeditText
                        text: search.placeholderText
                        color: search.placeholderTextColor
                        font: search.font
                        anchors { left: parent.left; leftMargin: search.leftPadding; verticalCenter: parent.verticalCenter }
                    }
                    Kbd {
                        text: "/"
                        visible: !search.activeFocus && search.text === ""
                        anchors { right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter }
                    }
                }
                onTextChanged: searchDelay.restart()
                Timer { id: searchDelay; interval: 250; onTriggered: page.applyFilter() }
                Keys.onEscapePressed: { text = ""; page.applyFilter(); page.focusGrid() }
                Keys.onReturnPressed: { page.applyFilter(); page.focusGrid() }
                Keys.onDownPressed: page.focusGrid()
            }

            Item { Layout.fillWidth: true }

            Rectangle {
                implicitWidth: seg.implicitWidth + 6
                implicitHeight: 40
                radius: 10
                color: Theme.surface
                Row {
                    id: seg
                    anchors.centerIn: parent
                    Repeater {
                        model: [{ label: "All", unwatched: false }, { label: "Unwatched", unwatched: true }]
                        delegate: T.Button {
                            required property var modelData
                            readonly property bool on: page.unwatchedOnly === modelData.unwatched
                            width: segLabel.implicitWidth + 28
                            height: 34
                            focusPolicy: Qt.NoFocus
                            onClicked: page.unwatchedOnly = modelData.unwatched
                            background: Rectangle { radius: 8; color: parent.on ? Theme.selected : "transparent" }
                            contentItem: Text {
                                id: segLabel
                                text: parent.modelData.label
                                color: parent.on ? Theme.text : Theme.text2
                                font.family: Theme.font
                                font.pixelSize: 13
                                font.weight: Font.Medium
                                horizontalAlignment: Text.AlignHCenter
                                verticalAlignment: Text.AlignVCenter
                            }
                        }
                    }
                }
            }

            SfButton {
                size: 40
                tone: Theme.surface
                iconName: "users"
                onClicked: page.manageRequested()
                QQC2.ToolTip.text: "Channels: add or remove (M)"
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: 600
            }

            SfButton {
                size: 40
                tone: Theme.surface
                iconName: "refresh-cw"
                text: page.feed.refreshing ? "Checking…" : page.feed.lastRefresh === "never" ? "Refresh" : "Checked " + page.feed.lastRefresh
                enabled: !page.feed.refreshing
                onClicked: page.feed.refresh()
                QQC2.ToolTip.text: "Check every channel for new videos (F5)"
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: 600
            }
        }

        ChannelChips {
            Layout.fillWidth: true
            Layout.preferredHeight: 44
            leftMargin: Theme.gutter
            rightMargin: Theme.gutter
            channels: page.channels
            selected: page.channelFilter
            shortsMode: page.shortsMode
            onPicked: (id, title) => page.selectChannel(id, title)
            onShortsToggled: page.shortsMode = !page.shortsMode
        }

        // yt-dlp notice: missing, or a newer one is out. "Later" hides it until the next release.
        Rectangle {
            id: ytdlpNotice
            readonly property bool show: page.ytdlp !== null && page.ytdlp.notice !== ""
                                         && (page.ytdlp.latest === "" || page.ytdlp.latest !== noticePrefs.dismissed)
            Settings { id: noticePrefs; category: "notices"; property string dismissed: "" }
            Layout.fillWidth: true
            Layout.leftMargin: Theme.gutter
            Layout.rightMargin: Theme.gutter
            Layout.topMargin: 10
            Layout.preferredHeight: show ? noticeRow.implicitHeight + 20 : 0
            visible: show
            radius: Theme.radiusM
            color: Qt.rgba(0.96, 0.65, 0.14, 0.08)
            border.color: Qt.rgba(0.96, 0.65, 0.14, 0.25)
            RowLayout {
                id: noticeRow
                anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; leftMargin: 16; rightMargin: 10 }
                spacing: 12
                Kirigami.Icon {
                    source: Theme.icon("triangle-alert")
                    isMask: true
                    color: Theme.warning
                    Layout.preferredWidth: 16
                    Layout.preferredHeight: 16
                }
                Text {
                    Layout.fillWidth: true
                    text: page.ytdlp ? page.ytdlp.notice + (page.ytdlp.hint !== "" ? " " + page.ytdlp.hint : "") : ""
                    color: Theme.text
                    font.family: Theme.font
                    font.pixelSize: 13
                    wrapMode: Text.Wrap
                }
                SfButton {
                    readonly property string command: page.ytdlp && page.ytdlp.hint.indexOf(": ") >= 0
                                                      ? page.ytdlp.hint.split(": ")[1].split(" (")[0] : ""
                    visible: command !== ""
                    size: 32
                    tone: Theme.raised
                    text: "Copy command"
                    onClicked: {
                        clipText.text = command
                        clipText.selectAll()
                        clipText.copy()
                        page.noticeCopied()
                    }
                    TextEdit { id: clipText; visible: false }
                }
                SfButton {
                    visible: page.ytdlp !== null && page.ytdlp.latest !== ""
                    size: 32
                    tone: Theme.raised
                    text: "Later"
                    onClicked: noticePrefs.dismissed = page.ytdlp.latest
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 2
            color: "transparent"
            QQC2.ProgressBar {
                anchors.fill: parent
                visible: page.feed.refreshing
                indeterminate: true
            }
        }

        GridView {
            id: grid

            readonly property int columns: Math.max(1, Math.floor((width - 2 * Theme.gutter + 24) / (page.shortsMode ? 200 : 300)))

            Layout.fillWidth: true
            Layout.fillHeight: true
            leftMargin: Theme.gutter
            rightMargin: Theme.gutter - 24
            topMargin: 18
            bottomMargin: 24
            clip: true
            focus: true
            model: page.feed
            keyNavigationEnabled: true
            highlightMoveDuration: 0
            cellWidth: Math.floor((width - leftMargin - rightMargin) / columns)
            cellHeight: Math.round((cellWidth - 24) * (page.shortsMode ? 16 / 9 : 9 / 16)) + 108
            boundsBehavior: Flickable.StopAtBounds
            QQC2.ScrollBar.vertical: QQC2.ScrollBar {}

            header: Column {
                width: grid.width - grid.leftMargin - grid.rightMargin - 24
                spacing: 0
                // The hero appearing or going away changes the header's height; GridView keeps
                // its old offset, which would leave the hero scrolled out of view.
                onHeightChanged: if (grid.currentIndex <= 0) Qt.callLater(grid.positionViewAtBeginning)
                topPadding: 14
                HeroSection {
                    width: parent.width
                    visible: page.hero !== null
                    height: visible ? implicitHeight : 0
                    video: page.hero
                    continuing: page.channelFilter === "" ? page.continuing : []
                    focused: page.heroFocused && grid.activeFocus
                    kicker: page.channelFilter === "" ? "Newest" : "Newest from " + page.channelFilterTitle
                    onPlay: (video) => page.playRequested(video)
                    onMarkWatched: page.feed.setHeroWatched(true)
                    onHide: page.hideHero()
                    onPointing: (video, on) => page.settle(video, on)
                }
                Row {
                    topPadding: page.hero !== null ? 40 : 4
                    bottomPadding: 16
                    spacing: 12
                    Text {
                        text: page.searchText !== "" ? (page.shortsMode ? "Shorts results" : "Results")
                            : page.shortsMode ? (page.channelFilter !== "" ? "Shorts from " + page.channelFilterTitle : "Shorts")
                            : page.channelFilter !== "" ? page.channelFilterTitle
                            : page.unwatchedOnly ? "Unwatched" : "Latest"
                        color: Theme.text
                        font.family: Theme.font
                        font.pixelSize: 20
                        font.weight: Font.DemiBold
                    }
                    Text {
                        anchors.baseline: parent.children[0].baseline
                        text: grid.count + (page.shortsMode ? (grid.count === 1 ? " Short" : " Shorts")
                                                             : (grid.count === 1 ? " video" : " videos"))
                        color: Theme.text3
                        font.family: Theme.font
                        font.pixelSize: 13
                    }
                }
            }

            delegate: VideoCard {
                width: grid.cellWidth
                height: grid.cellHeight
                focused: GridView.isCurrentItem && grid.activeFocus && !page.heroFocused
                onSelected: { page.heroFocused = false; grid.currentIndex = index; grid.forceActiveFocus() }
                onActivated: page.playRequested(page.feed.get(index) !== "" ? JSON.parse(page.feed.get(index)) : null)
                onContextMenuRequested: menu.popup()
                onPointing: (on) => page.settle({ videoId: videoId }, on)
            }

            // Not `move`: GridView already has a `move` transition property.
            function moveBy(step) {
                if (count > 0) currentIndex = Math.max(0, Math.min(count - 1, currentIndex + step))
            }
            function current() {
                if (page.heroFocused) return page.hero
                return currentIndex >= 0 && count > 0 ? JSON.parse(page.feed.get(currentIndex)) : null
            }

            Keys.onPressed: (event) => {
                const v = current()
                const atTop = currentIndex < columns
                switch (event.key) {
                case Qt.Key_J:
                case Qt.Key_Down:
                    if (page.heroFocused) { page.heroFocused = false; currentIndex = 0 }
                    else if (event.key === Qt.Key_J) moveBy(1)
                    else moveBy(columns)
                    break
                case Qt.Key_K:
                case Qt.Key_Up:
                    if (page.heroFocused) break
                    if ((event.key === Qt.Key_K ? currentIndex === 0 : atTop) && page.hero) {
                        page.heroFocused = true
                        positionViewAtBeginning()
                    } else moveBy(event.key === Qt.Key_K ? -1 : -columns)
                    break
                case Qt.Key_Left: if (!page.heroFocused) moveBy(-1); break
                case Qt.Key_Right: if (!page.heroFocused) moveBy(1); break
                case Qt.Key_Return:
                case Qt.Key_Enter:
                    if (v) page.playRequested(v)
                    break
                case Qt.Key_W:
                    if (page.heroFocused) page.feed.setHeroWatched(true)
                    else if (v) {
                        page.feed.setWatched(currentIndex, !v.watched)
                        if (!v.watched) moveBy(1)
                    }
                    break
                case Qt.Key_H:
                    if (page.heroFocused) page.hideHero()
                    else if (v) hideCurrent()
                    break
                case Qt.Key_U: page.unwatchedOnly = !page.unwatchedOnly; break
                case Qt.Key_S: page.shortsMode = !page.shortsMode; break
                case Qt.Key_M: page.manageRequested(); break
                case Qt.Key_C:
                    if (page.channelFilter !== "") page.selectChannel("", "")
                    else if (v) page.selectChannel(v.channelId, v.channelTitle)
                    break
                case Qt.Key_Slash: search.forceActiveFocus(); break
                case Qt.Key_F5: page.feed.refresh(); break
                case Qt.Key_Escape:
                    if (search.text !== "") { search.text = ""; page.applyFilter() }
                    else if (page.channelFilter !== "") page.selectChannel("", "")
                    else if (page.shortsMode) page.shortsMode = false
                    else return
                    break
                default: return
                }
                event.accepted = true
                switch (event.key) {
                case Qt.Key_J: case Qt.Key_K: case Qt.Key_Up: case Qt.Key_Down: case Qt.Key_Left: case Qt.Key_Right:
                    Qt.callLater(() => page.settle(current(), true))
                }
            }

            function hideCurrent() {
                const v = current()
                const index = currentIndex
                const id = page.feed.hide(index)
                if (id !== "") {
                    currentIndex = Math.min(index, count - 1)
                    page.hidden(id, v.title, false)
                }
            }

            QQC2.Menu {
                id: menu
                QQC2.MenuItem { icon.source: Theme.icon("play"); text: "Play"; onTriggered: page.playRequested(grid.current()) }
                QQC2.MenuItem {
                    icon.source: Theme.icon("check")
                    text: grid.currentItem && grid.currentItem.watched ? "Mark unwatched" : "Mark watched"
                    onTriggered: page.feed.setWatched(grid.currentIndex, !grid.currentItem.watched)
                }
                QQC2.MenuItem { icon.source: Theme.icon("eye-off"); text: "Hide"; onTriggered: grid.hideCurrent() }
                QQC2.MenuItem {
                    icon.source: Theme.icon("funnel")
                    text: "Only this channel"
                    onTriggered: page.selectChannel(grid.currentItem.channelId, grid.currentItem.channelTitle)
                }
            }

            Column {
                anchors.horizontalCenter: parent.horizontalCenter
                y: (grid.headerItem ? grid.headerItem.height : 0) + 60
                width: Math.min(parent.width - 80, 460)
                spacing: 10
                visible: grid.count === 0 && page.hero === null && !page.feed.refreshing
                Text {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    text: page.feed.channelCount === 0 ? "No subscriptions yet"
                        : page.searchText !== "" ? "Nothing matches “" + page.searchText + "”"
                        : page.unwatchedOnly ? "All caught up"
                        : page.shortsMode ? "No Shorts" : "No videos yet"
                    color: Theme.text
                    font.family: Theme.font
                    font.pixelSize: 22
                    font.weight: Font.DemiBold
                }
                Text {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    text: page.feed.channelCount === 0 ? "Import your Google Takeout export:\nsubfeed --import subscriptions.csv"
                        : page.unwatchedOnly ? "Every video here is watched. Press U to show them."
                        : page.searchText !== "" ? ""
                        : page.shortsMode ? "None of these channels has posted a Short lately. Press S to go back."
                        : "Press F5 to check for new videos."
                    color: Theme.text2
                    font.family: Theme.font
                    font.pixelSize: 14
                }
            }
        }
    }

    function hideHero() {
        const v = hero
        const id = feed.hideHero()
        if (id !== "") hidden(id, v.title, true)
    }
}
