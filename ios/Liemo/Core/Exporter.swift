import Accelerate
import AVFoundation
import Foundation
import Photos

/// Renders the open project with the engine and encodes it with AVAssetWriter
/// (H.264 up to 1080p, HEVC above), with the engine's mix as AAC.
struct ExportSettings {
    var resolution = 1080   // short side
    var fps = 30
}

enum ExportError: LocalizedError {
    case empty, writer(String), cancelled, photos
    var errorDescription: String? {
        switch self {
        case .empty: return "There's nothing on the timeline to export yet."
        case .writer(let m): return "The video couldn't be written: \(m)"
        case .cancelled: return "Export cancelled."
        case .photos: return "Liemo can't save to Photos. Allow it in Settings › Privacy › Photos."
        }
    }
}

final class Exporter: @unchecked Sendable {
    private let engine = Core.shared.engine
    private var cancelled = false
    func cancel() { cancelled = true }

    /// Writes the movie and returns its file.
    func run(doc: ProjectDoc, settings: ExportSettings, progress: @escaping @Sendable (Double) -> Void) throws -> URL {
        let duration = doc.duration
        guard duration > 0.05 else { throw ExportError.empty }
        let (cw, ch) = (doc.canvas.width, doc.canvas.height)
        let k = Double(settings.resolution) / min(cw, ch)
        let w = Int((cw * k / 2).rounded()) * 2, h = Int((ch * k / 2).rounded()) * 2
        let long = UInt32(max(w, h))

        let url = FileManager.default.temporaryDirectory.appendingPathComponent("Liemo-\(UUID().uuidString).mp4")
        let writer = try AVAssetWriter(outputURL: url, fileType: .mp4)
        let hevc = settings.resolution > 1080
        let bitrate = Double(w * h * settings.fps) * (hevc ? 0.07 : 0.1)
        let video = AVAssetWriterInput(mediaType: .video, outputSettings: [
            AVVideoCodecKey: hevc ? AVVideoCodecType.hevc : AVVideoCodecType.h264,
            AVVideoWidthKey: w,
            AVVideoHeightKey: h,
            AVVideoCompressionPropertiesKey: [AVVideoAverageBitRateKey: bitrate, AVVideoExpectedSourceFrameRateKey: settings.fps],
            AVVideoColorPropertiesKey: [
                AVVideoColorPrimariesKey: AVVideoColorPrimaries_ITU_R_709_2,
                AVVideoTransferFunctionKey: AVVideoTransferFunction_ITU_R_709_2,
                AVVideoYCbCrMatrixKey: AVVideoYCbCrMatrix_ITU_R_709_2,
            ],
        ])
        video.expectsMediaDataInRealTime = false
        let adaptor = AVAssetWriterInputPixelBufferAdaptor(assetWriterInput: video, sourcePixelBufferAttributes: [
            kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
            kCVPixelBufferWidthKey as String: w,
            kCVPixelBufferHeightKey as String: h,
        ])
        writer.add(video)

        let mix = engine.renderAudioRange(start: 0, stop: duration)
        var audio: AVAssetWriterInput?
        if mix.contains(where: { abs($0) > 1e-5 }) {
            let a = AVAssetWriterInput(mediaType: .audio, outputSettings: [
                AVFormatIDKey: kAudioFormatMPEG4AAC, AVSampleRateKey: 48000, AVNumberOfChannelsKey: 2, AVEncoderBitRateKey: 192_000,
            ])
            a.expectsMediaDataInRealTime = false
            writer.add(a)
            audio = a
        }

        guard writer.startWriting() else { throw ExportError.writer(writer.error?.localizedDescription ?? "unknown") }
        writer.startSession(atSourceTime: .zero)

        let frames = Int((duration * Double(settings.fps)).rounded(.up))
        let audioFrames = mix.count / 2
        var audioPos = 0
        let chunk = 4800
        var asbd = AudioStreamBasicDescription(mSampleRate: 48000, mFormatID: kAudioFormatLinearPCM,
                                               mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
                                               mBytesPerPacket: 8, mFramesPerPacket: 1, mBytesPerFrame: 8, mChannelsPerFrame: 2, mBitsPerChannel: 32, mReserved: 0)
        var afmt: CMAudioFormatDescription?
        CMAudioFormatDescriptionCreate(allocator: nil, asbd: &asbd, layoutSize: 0, layout: nil, magicCookieSize: 0, magicCookie: nil, extensions: nil, formatDescriptionOut: &afmt)

        func wait(_ input: AVAssetWriterInput) throws {
            while !input.isReadyForMoreMediaData {
                if cancelled { throw ExportError.cancelled }
                if writer.status == .failed { throw ExportError.writer(writer.error?.localizedDescription ?? "failed") }
                usleep(2000)
            }
        }

        /// Audio up to `until` seconds, so the writer gets both streams interleaved.
        func feedAudio(until: Double) throws {
            guard let a = audio, let afmt else { return }
            let limit = min(audioFrames, Int(until * 48000))
            while audioPos < limit {
                try wait(a)
                let n = min(chunk, audioFrames - audioPos)
                let bytes = n * 8
                var block: CMBlockBuffer?
                CMBlockBufferCreateWithMemoryBlock(allocator: nil, memoryBlock: nil, blockLength: bytes, blockAllocator: nil, customBlockSource: nil, offsetToData: 0, dataLength: bytes, flags: kCMBlockBufferAssureMemoryNowFlag, blockBufferOut: &block)
                guard let block else { break }
                mix.withUnsafeBytes { raw in
                    _ = CMBlockBufferReplaceDataBytes(with: raw.baseAddress! + audioPos * 8, blockBuffer: block, offsetIntoDestination: 0, dataLength: bytes)
                }
                var sb: CMSampleBuffer?
                CMAudioSampleBufferCreateReadyWithPacketDescriptions(allocator: nil, dataBuffer: block, formatDescription: afmt, sampleCount: n,
                                                                     presentationTimeStamp: CMTime(value: CMTimeValue(audioPos), timescale: 48000),
                                                                     packetDescriptions: nil, sampleBufferOut: &sb)
                if let sb { a.append(sb) }
                audioPos += n
            }
        }

        for i in 0..<frames {
            if cancelled { writer.cancelWriting(); throw ExportError.cancelled }
            let t = Double(i) / Double(settings.fps)
            try feedAudio(until: t + 0.5)
            guard let f = engine.renderFrame(time: t, max: long), let pool = adaptor.pixelBufferPool else { continue }
            var pb: CVPixelBuffer?
            CVPixelBufferPoolCreatePixelBuffer(nil, pool, &pb)
            guard let pb else { continue }
            copy(f, into: pb, w: w, h: h)
            try wait(video)
            adaptor.append(pb, withPresentationTime: CMTime(value: CMTimeValue(i), timescale: CMTimeScale(settings.fps)))
            progress(Double(i + 1) / Double(frames))
        }
        try feedAudio(until: .infinity)
        video.markAsFinished()
        audio?.markAsFinished()
        let sem = DispatchSemaphore(value: 0)
        writer.finishWriting { sem.signal() }
        sem.wait()
        guard writer.status == .completed else { throw ExportError.writer(writer.error?.localizedDescription ?? "failed") }
        return url
    }

    /// The engine's RGBA into the writer's BGRA buffer (scaled if the sizes differ by rounding).
    private func copy(_ f: MediaFrame, into pb: CVPixelBuffer, w: Int, h: Int) {
        CVPixelBufferLockBaseAddress(pb, [])
        defer { CVPixelBufferUnlockBaseAddress(pb, []) }
        var dst = vImage_Buffer(data: CVPixelBufferGetBaseAddress(pb), height: vImagePixelCount(h), width: vImagePixelCount(w), rowBytes: CVPixelBufferGetBytesPerRow(pb))
        var data = f.rgba
        data.withUnsafeMutableBytes { raw in
            var src = vImage_Buffer(data: raw.baseAddress, height: vImagePixelCount(f.height), width: vImagePixelCount(f.width), rowBytes: Int(f.width) * 4)
            let map: [UInt8] = [2, 1, 0, 3]
            if Int(f.width) == w && Int(f.height) == h {
                vImagePermuteChannels_ARGB8888(&src, &dst, map, vImage_Flags(kvImageNoFlags))
            } else {
                vImagePermuteChannels_ARGB8888(&src, &src, map, vImage_Flags(kvImageNoFlags))
                vImageScale_ARGB8888(&src, &dst, nil, vImage_Flags(kvImageHighQualityResampling))
            }
        }
    }

    static func saveToPhotos(_ url: URL) async throws {
        let status = await PHPhotoLibrary.requestAuthorization(for: .addOnly)
        guard status == .authorized || status == .limited else { throw ExportError.photos }
        try await PHPhotoLibrary.shared().performChanges {
            PHAssetCreationRequest.creationRequestForAssetFromVideo(atFileURL: url)
        }
    }
}
