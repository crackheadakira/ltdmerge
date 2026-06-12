use anyhow::Result;
use std::{cmp::max, path::Path};

#[inline]
fn align_up(size: u64, alignment: u64) -> u64 {
    (size + (alignment - 1)) & !(alignment - 1)
}

pub fn alloc_size(romfs_name: &str, len: usize) -> Result<u32> {
    let size = align_up(len as u64, 32);
    let path = Path::new(romfs_name);

    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase())
        .unwrap_or_default();

    let filename_lower = path
        .file_name()
        .and_then(|f| f.to_str())
        .map(|f| f.to_lowercase())
        .unwrap_or_default();

    let final_size: i64 = match romfs_name {
        "VoiceText/userdict_jpn.csv" => (size + 312) as i64,
        "Font/Font.Nin_NX_NVN.bfarc" => (size + 2592) as i64,
        "Sound/Resource/Mii_Static.bars" => (size + 640) as i64,
        "Tex/Pack/MiiFaceMaskPos.bntx" => (size + 3648) as i64,
        "Tex/Pack/MiiParts.bntx" => (size + 2400) as i64,

        _ => match extension.as_str() {
            "bgyml" => {
                size as i64 + BgymlResourceCalculator::calculate_offset(romfs_name, &filename_lower)
            }
            "blarc" => size as i64 + BlarcResourceCalculator::calculate_offset(&filename_lower),
            "bfres" => BfresResourceCalculator::calculate_size(size),
            "asb" => size as i64 + AsbResourceCalculator::calculate_offset(size),
            "byml" => size as i64 + BymlResourceCalculator::calculate_offset(&filename_lower),
            "bntx" => size as i64 + BntxResourceCalculator::calculate_offset(&filename_lower),
            "bfarc" => size as i64 + BfarcResourceCalculator::calculate_offset(&filename_lower),

            "ainb" => (size + 504) as i64,
            "baatarc" => (size + 288) as i64,
            "baev" => (size + 320) as i64,
            "bagst" => (size + 288) as i64,
            "bars" => (size + 608) as i64,
            "belnk" => (size + 288) as i64,
            "bfsha" => (size + 288) as i64,
            "bhtmp" => (size + 256) as i64,
            "blwp" => (size + 256) as i64,
            "bnsh" => (size + 2504) as i64,
            "bphcl" => (size + 1816) as i64,
            "bphhb" => (size + 256) as i64,
            "bphnm" => (size + 288) as i64,
            "bphsh" => (size + 400) as i64,
            "bslnk" => (size + 288) as i64,
            "genvb" => (size + 3736 + 0x1000) as i64,
            "pack" => (size + 384 + 0x180) as i64,
            "sarc" => (size + 8192) as i64,
            "txt" => (size + 288) as i64,
            "bin" => (size + 288) as i64,
            "vtdb2" => (size + 288) as i64,
            "csv" => (size + 288) as i64,

            "bwav" => -1,
            _ => ((size + 1500) * 4) as i64,
        },
    };

    Ok(final_size as u32)
}

struct BgymlResourceCalculator;
impl BgymlResourceCalculator {
    fn calculate_offset(romfs_name: &str, filename_lower: &str) -> i64 {
        if filename_lower.ends_with(".alto__altoconfig.bgyml") {
            return 2832;
        }

        if romfs_name.ends_with("ui__SystemParam.bgyml") {
            return 5480;
        }

        if romfs_name.ends_with("ui__SnapshotCamera.bgyml") {
            return 864;
        }

        if romfs_name.ends_with("ui__Parts3DLayoutParam.bgyml") {
            return 1024 + 8;
        }

        if romfs_name.ends_with("ui__MessageSystem.bgyml") {
            return 1024;
        }

        if romfs_name.ends_with("sound__VoicePlayParam.bgyml") {
            return 7384;
        }

        if romfs_name.ends_with("sound__VoiceLanguageOffset.bgyml") {
            return 1800;
        }

        if romfs_name.ends_with("sound__OutputDeviceSetting.bgyml") {
            return 904;
        }

        if romfs_name.ends_with("sound__LeakOutSetting.bgyml") {
            return 1840;
        }

        if romfs_name.ends_with("sound__LeakOutParam.bgyml") {
            return 1024 + 16;
        }

        if romfs_name.ends_with("sound__IgnoreDuckingSetting.bgyml") {
            return 992;
        }

        if romfs_name.ends_with("sound__FaderDuckingParam.bgyml") {
            return 1248;
        }

        if romfs_name.ends_with("sound__ChimeSetting.bgyml") {
            return 2080;
        }

        if romfs_name.ends_with("sound__CameraLinkSetting.bgyml") {
            return 968;
        }

        if romfs_name.ends_with("sound__AIBgmCtrlParam.bgyml") {
            return 864;
        }

        if romfs_name.ends_with("pp__CombinationDataTableData.bgyml") {
            return 13960;
        }

        if romfs_name.ends_with("phive__RigidBodyEntityParam.bgyml") {
            return 1112;
        }

        if romfs_name.ends_with("phive__RigidBodyControllerEntityParam.bgyml") {
            return 1600;
        }

        if romfs_name.ends_with("gfx__OceanSystemParam.bgyml") {
            return 1392;
        }

        if romfs_name.ends_with("actor__ActorColorVariationSetting.bgyml") {
            return 864;
        }

        if romfs_name.ends_with("actor__AccidentSystemParam.bgyml") {
            return 864;
        }

        if filename_lower.contains("bodyhelperbone") {
            return 288 + 0x7C28;
        }

        if filename_lower.contains("miinewsparam") {
            return 288 + 0x2070;
        }

        if filename_lower.contains("focusleading") {
            return 288 + 0x19A8;
        }

        if filename_lower.contains("phive__") {
            return 288 + 0x1590 + 576;
        }

        if filename_lower.contains("actorparam") || filename_lower.contains("decoparam") {
            return 288 + 0xFA8 + 576;
        }

        if filename_lower.contains("parameter") || filename_lower.contains("component") {
            return 288 + 0x3568 + 576;
        }

        2900
    }
}

struct BlarcResourceCalculator;
impl BlarcResourceCalculator {
    fn calculate_offset(filename_lower: &str) -> i64 {
        if filename_lower.contains("menu_") || filename_lower.contains("layout/") {
            return 656 + 0x4CE8;
        }

        4504
    }
}

struct BfresResourceCalculator;
impl BfresResourceCalculator {
    fn calculate_size(size: u64) -> i64 {
        let base_padding = size + 0x12718;
        let scaled_padding = size + std::cmp::min((size as f64 * 1.5) as u64 + 0x18000, 0x300000);
        max(base_padding, scaled_padding) as i64
    }
}

struct AsbResourceCalculator;
impl AsbResourceCalculator {
    fn calculate_offset(size: u64) -> i64 {
        std::cmp::min((size as f64 * 0.5) as u64 + 2048, 750_000) as i64
    }
}

struct BymlResourceCalculator;
impl BymlResourceCalculator {
    fn calculate_offset(filename_lower: &str) -> i64 {
        if filename_lower.contains("rstbl") {
            return 288 + 0x6A9E0;
        }

        if filename_lower.contains("esetb.byml") {
            return 288 + 0x220;
        }

        288
    }
}

struct BntxResourceCalculator;
impl BntxResourceCalculator {
    fn calculate_offset(filename_lower: &str) -> i64 {
        if filename_lower.contains("miieditoricon") {
            return 4096 + 0xB4E0;
        }

        4096
    }
}

struct BfarcResourceCalculator;
impl BfarcResourceCalculator {
    fn calculate_offset(filename_lower: &str) -> i64 {
        if filename_lower.contains("font") {
            return 288 + 0x900;
        }

        288
    }
}
