#pragma once

#include <QtCore/QObject>
#include <QtCore/QStringList>
#include <QtDBus/QDBusMessage>
#include <QtQml/qqmlregistration.h>

// What the player needs to know about this computer: which codecs the GPU decodes (VA-API),
// and the power state (UPower, power-profiles-daemon), kept up to date over D-Bus.
class SystemInfo : public QObject
{
    Q_OBJECT
    QML_ELEMENT
    QML_SINGLETON
    // Video codecs VA-API can decode here: any of "av1", "vp9", "h264". Empty when there is no
    // VA-API device (e.g. NVIDIA without nvidia-vaapi-driver); mpv may still decode via NVDEC.
    Q_PROPERTY(QStringList decodableCodecs READ decodableCodecs CONSTANT)
    Q_PROPERTY(bool onBattery READ onBattery NOTIFY powerChanged)
    // power-profiles-daemon's "power-saver" profile is active.
    Q_PROPERTY(bool powerSaver READ powerSaver NOTIFY powerChanged)

public:
    explicit SystemInfo(QObject *parent = nullptr);

    // mpv's `hwdec` list for this machine. Hybrid NVIDIA laptops get VA-API only, so decoding
    // never wakes the discrete GPU; everything else tries VA-API, then NVDEC, then any safe one.
    static QString preferredHwdec();
    // mpv's `gpu-hwdec-interop`. On hybrid NVIDIA laptops only VA-API's is loaded: mpv otherwise
    // initialises the CUDA interop at startup, which opens /dev/nvidiactl and wakes the dGPU
    // (measured). Elsewhere "auto".
    static QString preferredHwdecInterop();

    QStringList decodableCodecs() const { return m_codecs; }
    bool onBattery() const { return m_onBattery; }
    bool powerSaver() const { return m_powerSaver; }

Q_SIGNALS:
    void powerChanged();

private Q_SLOTS:
    void onPropertiesChanged(const QString &interface, const QVariantMap &changed, const QStringList &invalidated);

private:
    void readPower();

    QStringList m_codecs;
    bool m_onBattery = false;
    bool m_powerSaver = false;
    QString m_profilesService;
};
