import QtCore
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Window
import dev.subfeed
import dev.subfeed.core

// subfeed's window: the feed, and the player pushed on top of it.
QQC2.ApplicationWindow {
    id: root

    property bool smokeTest: false
    // --open: start in the player. --screenshot: save this window as PNG, then quit.
    property string openVideoId: ""
    // --shorts: start in the Shorts view.
    property bool startInShorts: false
    // --manage: start on the Channels page.
    property bool startInChannels: false
    // --tour: walks through the app by itself, then quits (for demo recordings).
    property bool tour: false
    property string screenshotPath: ""
    property int screenshotScale: 1
    // AppBus (C++): later launches of subfeed hand over to this window.
    property QtObject appBus: null
    // Null until the pushed page has its video, so bindings never see it half set up.
    // First run: no channels yet.
    property bool welcome: false
    // The player is always dark (Theme.qml).
    Binding { target: Theme; property: "forceDark"; value: root.playerPage !== null }
    // SUBFEED_COLOR_SCHEME: light or dark regardless of the desktop (screenshots, tests).
    property string colorSchemeOverride: ""
    Binding { target: Theme; property: "override"; value: root.colorSchemeOverride }
    readonly property var playerPage: stack.depth > 1 && stack.currentItem && stack.currentItem.video ? stack.currentItem : null
    property int restoreVisibility: Window.Windowed

    title: root.playerPage && playerSession.title !== "" ? playerSession.title + " — subfeed" : "subfeed"
    width: 1280
    height: 800
    minimumWidth: 720
    minimumHeight: 480
    visible: true
    color: Theme.bg
    font.family: Theme.font
    // For the stock controls (Basic style): menus, scrollbars, tooltips, progress bar.
    palette {
        window: Theme.surface
        windowText: Theme.text
        base: Theme.raised
        alternateBase: Theme.surface
        text: Theme.text
        button: Theme.raised
        buttonText: Theme.text
        brightText: Theme.text
        highlight: Theme.accent
        highlightedText: Theme.accentInk
        light: Theme.hover
        midlight: Theme.hover
        mid: Theme.mid
        dark: Theme.bg
        shadow: "#000000"
        toolTipBase: Theme.popup
        toolTipText: Theme.text
        placeholderText: Theme.text3
    }

    FeedModel {
        id: feedModel
        onChanged: channelModel.reload()
        onStatusChanged: if (status !== "") toast.show(status)
    }
    ChannelModel { id: channelModel }
    YtdlpStatus {
        id: ytdlpStatus
        onUpdatedChanged: if (updated !== "") toast.show("yt-dlp updated to " + updated)
    }
    // Flatpak: offered once, after the first channels come in.
    BackgroundRefresh {
        id: backgroundRefresh
        onFailed: (reason) => toast.show(reason)
    }
    Settings { id: offers; category: "offers"; property bool background: false }
    Timer {
        id: offerBackground
        interval: 5000
        onTriggered: {
            if (!backgroundRefresh.available || backgroundRefresh.enabled || offers.background) return
            offers.background = true
            toast.show("Check for new videos while subfeed is closed?", "Allow", () => backgroundRefresh.enable())
        }
    }
    PlayerSession { id: playerSession }

    Settings {
        property alias unwatchedOnly: feedPage.unwatchedOnly
        property alias channelFilter: feedPage.channelFilter
        property alias channelFilterTitle: feedPage.channelFilterTitle
    }

    // Plays a video by id: from the feed if it's there, otherwise with what we know.
    function openById(id) {
        const row = feedModel.rowOf(id)
        const hero = feedModel.heroJson !== "" ? JSON.parse(feedModel.heroJson) : null
        play(row >= 0 ? JSON.parse(feedModel.get(row))
             : hero && hero.videoId === id ? hero
             : { videoId: id, title: "", channelTitle: "", thumbnail: "" })
    }
    function bringToFront() {
        if (visibility === Window.Minimized || visibility === Window.Hidden) showNormal()
        raise()
        requestActivate()
    }

    function play(video) {
        if (!video) return
        if (stack.depth > 1) stack.pop(null, QQC2.StackView.Immediate)
        stack.push(playerComponent, { video: video })
    }
    function closePlayer() {
        if (visibility === Window.FullScreen) visibility = restoreVisibility
        stack.pop()
        player.parent = root.contentItem
        player.opacity = 0
        feedModel.reload()
        channelModel.reload()
        feedPage.focusGrid()
    }
    function toggleFullscreen() {
        if (visibility === Window.FullScreen) {
            visibility = restoreVisibility
        } else {
            restoreVisibility = visibility
            visibility = Window.FullScreen
        }
    }

    QQC2.StackView {
        id: stack
        anchors.fill: parent
        initialItem: FeedPage {
            id: feedPage
            feed: feedModel
            channels: channelModel
            onPlayRequested: (video) => root.play(video)
            onManageRequested: stack.push(channelsComponent)
            ytdlp: ytdlpStatus
            onNoticeCopied: toast.show("Copied; paste it into a terminal")
            onHidden: (videoId, title, isHero) => toast.show("Hid “" + title + "”", "Undo", () => feedModel.unhide(videoId))
        }
        pushEnter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 160 } }
        pushExit: Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: 160 } }
        popEnter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 160 } }
        popExit: Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: 160 } }
    }

    Component {
        id: channelsComponent
        ChannelsPage {
            channels: channelModel
            onBack: (added, removed, channelId, title) => {
                stack.pop()
                if (channelId !== "") {
                    // A channel was clicked: its videos (not its Shorts).
                    feedPage.shortsMode = false
                    feedPage.selectChannel(channelId, title)
                } else if (removed.indexOf(feedPage.channelFilter) >= 0) {
                    // A removed channel can't stay the filter.
                    feedPage.selectChannel("", "")
                }
                feedModel.reload()
                channelModel.reload()
                feedPage.focusGrid()
                if (added) feedModel.refresh()
            }
        }
    }

    Component {
        id: playerComponent
        PlayerPage {
            mpv: player
            session: playerSession
            feed: feedModel
            fullscreen: root.visibility === Window.FullScreen
            onBack: root.closePlayer()
            onFullscreenToggled: root.toggleFullscreen()
            onPlayOther: (video) => {
                if (player.loaded) playerSession.savePosition(player.position)
                player.stop()
                root.play(video)
            }
        }
    }

    Loader {
        id: welcomeLoader
        anchors.fill: parent
        active: root.welcome
        focus: active
        sourceComponent: WelcomePage {
            focus: true
            onDone: (imported) => {
                root.welcome = false
                feedModel.reload()
                channelModel.reload()
                feedPage.focusGrid()
                feedModel.refresh()
                if (imported > 0) toast.show("Following " + feedModel.channelCount + " channels")
                offerBackground.start()
            }
        }
    }

    Toast { id: toast }

    Connections {
        target: root.appBus
        function onActivateRequested() { root.bringToFront() }
        function onOpenRequested(videoId) { root.openById(videoId); root.bringToFront() }
    }

    // Media keys and desktop media widgets.
    Mpris {
        player: player
        active: root.playerPage !== null
        videoId: root.playerPage ? root.playerPage.videoId : ""
        title: root.playerPage ? root.playerPage.displayTitle : ""
        artist: playerSession.channel !== "" ? playerSession.channel : (root.playerPage ? root.playerPage.video.channelTitle : "")
        artUrl: root.playerPage ? (root.playerPage.video.hero || root.playerPage.video.thumbnail
                                   || "https://i.ytimg.com/vi/" + root.playerPage.videoId + "/hqdefault.jpg") : ""
        canGoNext: root.playerPage !== null && root.playerPage.upNext.length > 0
        onNextRequested: if (root.playerPage && root.playerPage.upNext.length > 0) root.playerPage.playOther(root.playerPage.upNext[0])
        onPreviousRequested: player.seekTo(0)
        onStopRequested: if (root.playerPage) root.playerPage.leave()
        onRaiseRequested: root.bringToFront()
        onQuitRequested: Qt.quit()
    }

    // No screen blanking or sleep while a video plays.
    Inhibitor { active: root.playerPage !== null && player.loaded && !player.paused }

    // Unplugged or plugged in mid-video: pick the stream for the new power state.
    Connections {
        target: SystemInfo
        function onPowerChanged() {
            if (root.playerPage && playerSession.state === "ready" && player.loaded)
                playerSession.adapt(SystemInfo.onBattery, SystemInfo.powerSaver,
                                    SystemInfo.decodableCodecs.join(","), player.position)
        }
    }

    // While open, check for new videos every 30 minutes, like the background timer (on battery
    // the Rust side reads only the RSS feeds, with no yt-dlp).
    Timer {
        interval: 30 * 60 * 1000
        repeat: true
        running: root.screenshotPath === "" && !root.smokeTest && !root.tour
        onTriggered: feedModel.refresh()
    }

    // The tour: each step's action, then how long to wait before the next (approximate: QML timers
    // follow the animation clock). The large video is looked up first, so the player opens at once.
    readonly property var tourSteps: [
        { wait: 2800, run: () => { if (feedModel.heroJson !== "") feedModel.prefetch(JSON.parse(feedModel.heroJson).videoId) } },
        { wait: 800, run: () => feedPage.moveSelection(1) },
        { wait: 800, run: () => feedPage.moveSelection(1) },
        { wait: 1800, run: () => feedPage.moveSelection(1) },
        { wait: 3200, run: () => { feedPage.shortsMode = true } },
        { wait: 1200, run: () => { feedPage.shortsMode = false } },
        { wait: 3000, run: () => stack.push(channelsComponent) },
        { wait: 1500, run: () => { stack.pop(); feedPage.focusGrid() } },
        { wait: 5500, run: () => play(JSON.parse(feedModel.heroJson)) },
        { wait: 3500, run: () => root.playerPage.toggleCaptions() },
        // Just before a sponsor segment, to show SponsorBlock skipping it.
        { wait: 6500, run: () => player.seekTo(Math.max(0, (root.playerPage.skips.length > 0 ? root.playerPage.skips[0].start : 60) - 3)) },
        { wait: 3500, run: () => root.playerPage.seekChapter(1) },
        { wait: 2500, run: () => root.playerPage.leave() },
        { wait: 0, run: () => Qt.quit() },
    ]
    property int tourStep: 0
    Timer {
        id: tourTimer
        interval: 1500
        running: root.tour
        onTriggered: {
            const step = root.tourSteps[root.tourStep++]
            step.run()
            if (root.tourStep < root.tourSteps.length) {
                interval = step.wait
                start()
            }
        }
    }

    Component.onCompleted: {
        channelModel.reload()
        feedPage.shortsMode = startInShorts
        feedPage.applyFilter()
        feedPage.focusGrid()
        welcome = feedModel.channelCount === 0
        ytdlpStatus.check()
        if (startInChannels && !welcome) stack.push(channelsComponent, {}, QQC2.StackView.Immediate)
        if (openVideoId !== "") openById(openVideoId)
        // Opened after a while away (or after the background timer couldn't run): catch up now.
        if (!welcome && screenshotPath === "" && !smokeTest && !tour) feedModel.refreshIfStale(15)
    }

    Timer {
        running: root.screenshotPath !== ""
        interval: root.openVideoId !== "" ? 26000 : 3000
        onTriggered: {
            if (root.openVideoId !== "") {
                console.log("subfeed: playing pos=" + player.position.toFixed(1) + " hwdec=" + player.hwdec + " codecs=" + SystemInfo.decodableCodecs + " power=" + SystemInfo.onBattery + "/" + SystemInfo.powerSaver
                            + " quality=" + playerSession.quality + " battery=" + playerSession.onBattery
                            + " buffered=" + player.bufferedAhead.toFixed(1))
                player.paused = true
            }
            grabDelay.start()
        }
    }
    Timer {
        id: grabDelay
        interval: 500
        // The welcome page when it's showing, else the page stack.
        function shotTarget() { return root.welcome ? welcomeLoader : stack }
        onTriggered: {
            const item = shotTarget()
            item.grabToImage((result) => {
                console.log("subfeed: screenshot " + (result.saveToFile(root.screenshotPath) ? "saved" : "FAILED"))
                Qt.quit()
            }, Qt.size(item.width * root.screenshotScale, item.height * root.screenshotScale))
        }
    }

    // One mpv instance for the app's lifetime; the player page borrows it.
    MpvItem {
        id: player
        width: 1
        height: 1
        opacity: 0

        onMpvVersionChanged: {
            console.log("subfeed: core=" + AppInfo.version + " player=" + mpvVersion
                        + " channels=" + feedModel.channelCount)
            if (root.smokeTest) Qt.quit()
        }
    }
}
