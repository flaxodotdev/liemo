// Remove background on the Mac: Apple's Vision finds the subject (whatever stands out in front,
// or failing that the people), as on iPhone and iPad.

use objc2::rc::Retained;
use objc2::AnyThread;
use objc2_core_foundation::CFData;
use objc2_core_graphics::{CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo};
use objc2_core_video::{
    kCVPixelFormatType_OneComponent8, kCVPixelFormatType_OneComponent16, kCVPixelFormatType_OneComponent32Float, CVPixelBuffer,
    CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight, CVPixelBufferGetPixelFormatType,
    CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSArray, NSDictionary, NSIndexSet, NSUInteger};
use objc2_vision::{
    VNGenerateForegroundInstanceMaskRequest, VNGeneratePersonSegmentationRequest, VNGeneratePersonSegmentationRequestQualityLevel, VNImageRequestHandler,
    VNRequest,
};
use tiny_skia::Pixmap;

/// The subject mask (0–255 per pixel) for a premultiplied RGBA frame, at the mask's own size.
/// With `pick` (normalised x, y in the frame), only the instance under that point is kept;
/// a miss or an empty pick falls back to every subject.
pub fn segment(pm: &Pixmap, pick: Option<(f32, f32)>) -> Option<(Vec<u8>, u32, u32)> {
    objc2::rc::autoreleasepool(|_| unsafe {
        let (w, h) = (pm.width() as usize, pm.height() as usize);
        let data = CFData::new(None, pm.data().as_ptr(), pm.data().len() as isize)?;
        let provider = CGDataProvider::with_cf_data(Some(&data))?;
        let space = CGColorSpace::new_device_rgb()?;
        let image = CGImage::new(
            w,
            h,
            8,
            32,
            w * 4,
            Some(&space),
            CGBitmapInfo(CGImageAlphaInfo::PremultipliedLast.0),
            Some(&provider),
            std::ptr::null(),
            true,
            CGColorRenderingIntent::RenderingIntentDefault,
        )?;
        let handler = VNImageRequestHandler::initWithCGImage_options(VNImageRequestHandler::alloc(), &image, &NSDictionary::new());

        let subjects = VNGenerateForegroundInstanceMaskRequest::new();
        let reqs: Retained<NSArray<VNRequest>> = NSArray::from_retained_slice(&[Retained::into_super(Retained::into_super(subjects.clone()))]);
        if handler.performRequests_error(&reqs).is_ok() {
            if let Some(obs) = subjects.results().and_then(|r| r.firstObject()) {
                let all = obs.allInstances();
                if all.count() > 0 {
                    // A picked point keeps only the instance under it.
                    if let Some((nx, ny)) = pick.and_then(|(x, y)| (x.is_finite() && y.is_finite()).then_some((x, y))) {
                        if let Some(inst) = instance_at(&obs, nx, ny) {
                            let one = NSIndexSet::indexSetWithIndex(inst);
                            if let Ok(buf) = obs.generateScaledMaskForImageForInstances_fromRequestHandler_error(&one, &handler) {
                                return read_mask(&buf);
                            }
                        }
                    }
                    if let Ok(buf) = obs.generateScaledMaskForImageForInstances_fromRequestHandler_error(&all, &handler) {
                        return read_mask(&buf);
                    }
                }
            }
        }
        let people = VNGeneratePersonSegmentationRequest::new();
        people.setQualityLevel(VNGeneratePersonSegmentationRequestQualityLevel::Balanced);
        let reqs: Retained<NSArray<VNRequest>> = NSArray::from_retained_slice(&[Retained::into_super(Retained::into_super(Retained::into_super(people.clone())))]);
        handler.performRequests_error(&reqs).ok()?;
        let obs = people.results()?.firstObject()?;
        read_mask(&obs.pixelBuffer())
    })
}

/// The instance index under a normalised point of the frame, read off the low-res instance
/// map (0 is the background). Instance numbers change from frame to frame, so the pick is a
/// point, re-read every frame.
unsafe fn instance_at(obs: &objc2_vision::VNInstanceMaskObservation, nx: f32, ny: f32) -> Option<NSUInteger> {
    let buf = obs.instanceMask();
    CVPixelBufferLockBaseAddress(&buf, CVPixelBufferLockFlags::ReadOnly);
    let (w, h, row) = (CVPixelBufferGetWidth(&buf), CVPixelBufferGetHeight(&buf), CVPixelBufferGetBytesPerRow(&buf));
    let base = CVPixelBufferGetBaseAddress(&buf) as *const u8;
    let fmt = CVPixelBufferGetPixelFormatType(&buf);
    let mut out = None;
    if !base.is_null() && w > 0 && h > 0 {
        let x = ((nx * w as f32) as usize).min(w as usize - 1);
        let y = ((ny * h as f32) as usize).min(h as usize - 1);
        let line = base.add(y * row);
        out = if fmt == kCVPixelFormatType_OneComponent8 {
            Some(*line.add(x) as NSUInteger)
        } else if fmt == kCVPixelFormatType_OneComponent16 {
            Some((*(line as *const u16).add(x)) as NSUInteger)
        } else if fmt == kCVPixelFormatType_OneComponent32Float {
            Some((*(line as *const f32).add(x)).round().max(0.0) as NSUInteger)
        } else {
            None
        };
    }
    CVPixelBufferUnlockBaseAddress(&buf, CVPixelBufferLockFlags::ReadOnly);
    out.filter(|&i| i != 0)
}

/// A one-channel mask buffer (8-bit or float) as bytes.
unsafe fn read_mask(buf: &CVPixelBuffer) -> Option<(Vec<u8>, u32, u32)> {
    CVPixelBufferLockBaseAddress(buf, CVPixelBufferLockFlags::ReadOnly);
    let (w, h, row) = (CVPixelBufferGetWidth(buf), CVPixelBufferGetHeight(buf), CVPixelBufferGetBytesPerRow(buf));
    let base = CVPixelBufferGetBaseAddress(buf) as *const u8;
    let float = CVPixelBufferGetPixelFormatType(buf) == kCVPixelFormatType_OneComponent32Float;
    let mut out = Vec::with_capacity(w * h);
    if !base.is_null() {
        for y in 0..h {
            let line = base.add(y * row);
            for x in 0..w {
                out.push(if float { ((*(line as *const f32).add(x)).clamp(0.0, 1.0) * 255.0) as u8 } else { *line.add(x) });
            }
        }
    }
    CVPixelBufferUnlockBaseAddress(buf, CVPixelBufferLockFlags::ReadOnly);
    (!base.is_null()).then_some((out, w as u32, h as u32))
}
