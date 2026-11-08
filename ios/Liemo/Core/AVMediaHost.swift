import AVFoundation
import CoreGraphics
import Foundation
import ImageIO

/// What the engine asks of AVFoundation and Image I/O: the desktop app does these with FFmpeg.
/// The engine calls in from its own threads and waits for the answer.
final class AVMediaHost: MediaHost, @unchecked Sendable {
    private let lock = NSLock()
    private var generators: [String: AVAssetImageGenerator] = [:]

    func probe(path: String) -> MediaProbe? {
        let asset = AVURLAsset(url: URL(fileURLWithPath: path))
        return blocking {
            let dur = try await asset.load(.duration).seconds
            let video = try await asset.loadTracks(withMediaType: .video).first
            let audio = try await asset.loadTracks(withMediaType: .audio).first
            var size = CGSize.zero
            if let v = video {
                let (natural, transform) = try await v.load(.naturalSize, .preferredTransform)
                let r = CGRect(origin: .zero, size: natural).applying(transform)
                size = CGSize(width: abs(r.width), height: abs(r.height))
            }
            return MediaProbe(duration: dur, width: UInt32(size.width), height: UInt32(size.height), hasVideo: video != nil, hasAudio: audio != nil)
        }
    }

    func videoFrame(path: String, time: Double, maxW: UInt32, maxH: UInt32) -> MediaFrame? {
        let key = "\(path)|\(maxW)x\(maxH)"
        lock.lock()
        let gen = generators[key] ?? {
            let g = AVAssetImageGenerator(asset: AVURLAsset(url: URL(fileURLWithPath: path)))
            g.appliesPreferredTrackTransform = true
            g.maximumSize = CGSize(width: Int(maxW), height: Int(maxH))
            let tol = CMTime(value: 1, timescale: 60)
            g.requestedTimeToleranceBefore = tol
            g.requestedTimeToleranceAfter = tol
            generators[key] = g
            return g
        }()
        lock.unlock()
        let at = CMTime(seconds: time, preferredTimescale: 600)
        guard let img = blocking({ try await gen.image(at: at).image }) else { return nil }
        return frame(of: img, max: nil)
    }

    func decodeImage(path: String, max: UInt32) -> MediaFrame? {
        let url = URL(fileURLWithPath: path) as CFURL
        guard let src = CGImageSourceCreateWithURL(url, nil) else { return nil }
        let opts: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: Int(max),
        ]
        guard let img = CGImageSourceCreateThumbnailAtIndex(src, 0, opts as CFDictionary) else { return nil }
        return frame(of: img, max: nil)
    }

    func decodeAudio(path: String) -> [Float]? {
        let asset = AVURLAsset(url: URL(fileURLWithPath: path))
        guard let track = blocking({ try await asset.loadTracks(withMediaType: .audio).first }) ?? nil,
              let reader = try? AVAssetReader(asset: asset) else { return nil }
        let out = AVAssetReaderTrackOutput(track: track, outputSettings: [
            AVFormatIDKey: kAudioFormatLinearPCM,
            AVSampleRateKey: 48000,
            AVNumberOfChannelsKey: 2,
            AVLinearPCMBitDepthKey: 32,
            AVLinearPCMIsFloatKey: true,
            AVLinearPCMIsNonInterleaved: false,
            AVLinearPCMIsBigEndianKey: false,
        ])
        guard reader.canAdd(out) else { return nil }
        reader.add(out)
        guard reader.startReading() else { return nil }
        var samples: [Float] = []
        while let buf = out.copyNextSampleBuffer() {
            guard let block = CMSampleBufferGetDataBuffer(buf) else { continue }
            let n = CMBlockBufferGetDataLength(block) / 4
            let start = samples.count
            samples.append(contentsOf: repeatElement(0, count: n))
            samples.withUnsafeMutableBytes { raw in
                _ = CMBlockBufferCopyDataBytes(block, atOffset: 0, dataLength: n * 4, destination: raw.baseAddress! + start * 4)
            }
        }
        return reader.status == .completed ? samples : nil
    }

    /// Draw a CGImage into RGBA bytes.
    private func frame(of img: CGImage, max: Int?) -> MediaFrame? {
        let w = img.width, h = img.height
        var data = Data(count: w * h * 4)
        let ok = data.withUnsafeMutableBytes { raw -> Bool in
            guard let ctx = CGContext(data: raw.baseAddress, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
                                      space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return false }
            ctx.draw(img, in: CGRect(x: 0, y: 0, width: w, height: h))
            return true
        }
        return ok ? MediaFrame(width: UInt32(w), height: UInt32(h), rgba: data) : nil
    }
}

/// Wait for async AVFoundation work from a synchronous engine callback.
func blocking<T>(_ body: @escaping @Sendable () async throws -> T) -> T? {
    let sem = DispatchSemaphore(value: 0)
    let box = Box<T>()
    Task.detached(priority: .userInitiated) {
        box.value = try? await body()
        sem.signal()
    }
    sem.wait()
    return box.value
}

private final class Box<T>: @unchecked Sendable { var value: T? }
