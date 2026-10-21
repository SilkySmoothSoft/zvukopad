//! Вшивает иконку `res/zvukopad.ico` в zvukopad.exe — её видно в Проводнике и на ярлыках.
//!
//! Обычно для этого нужен компилятор ресурсов (windres или rc.exe). Чтобы сборка не зависела
//! от лишних программ, объектный файл с ресурсом (COFF, секция .rsrc) собирается прямо здесь:
//! это простая таблица «тип → номер → язык → данные». Формат описан в документации Microsoft
//! «PE Format», раздел «The .rsrc Section».
//!
//! Иконку можно заменить: положите свой .ico в app/res/zvukopad.ico и пересоберите.

use std::path::PathBuf;

const RT_ICON: u32 = 3;
const RT_GROUP_ICON: u32 = 14;

fn main() {
    println!("cargo:rerun-if-changed=res/zvukopad.ico");
    println!("cargo:rerun-if-changed=build.rs");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    if os != "windows" {
        return;
    }
    if env != "gnu" || arch != "x86_64" {
        println!("cargo:warning=иконка exe вшивается только для x86_64-pc-windows-gnu");
        return;
    }
    let ico = match std::fs::read("res/zvukopad.ico") {
        Ok(b) => b,
        Err(e) => {
            println!("cargo:warning=нет res/zvukopad.ico ({e}) — exe будет без иконки");
            return;
        }
    };
    let images = match parse_ico(&ico) {
        Ok(v) => v,
        Err(e) => {
            println!("cargo:warning=res/zvukopad.ico повреждён: {e} — exe будет без иконки");
            return;
        }
    };
    let obj = build_coff(&images);
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("zvukopad-icon.o");
    std::fs::write(&out, obj).expect("не удалось записать объект с иконкой");
    println!("cargo:rustc-link-arg-bins={}", out.display());
}

/// Одна картинка из .ico: поля заголовка и данные (BMP или PNG как есть).
struct IconImage {
    width: u8,
    height: u8,
    colors: u8,
    planes: u16,
    bit_count: u16,
    data: Vec<u8>,
}

fn u16_at(b: &[u8], i: usize) -> Result<u16, String> {
    b.get(i..i + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).ok_or_else(|| "файл обрезан".into())
}
fn u32_at(b: &[u8], i: usize) -> Result<u32, String> {
    b.get(i..i + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]])).ok_or_else(|| "файл обрезан".into())
}

fn parse_ico(b: &[u8]) -> Result<Vec<IconImage>, String> {
    if u16_at(b, 0)? != 0 || u16_at(b, 2)? != 1 {
        return Err("это не .ico".into());
    }
    let count = u16_at(b, 4)? as usize;
    if count == 0 {
        return Err("в файле нет картинок".into());
    }
    let mut v = Vec::with_capacity(count);
    for i in 0..count {
        let e = 6 + i * 16;
        let size = u32_at(b, e + 8)? as usize;
        let offset = u32_at(b, e + 12)? as usize;
        let data = b.get(offset..offset + size).ok_or("картинка за пределами файла")?.to_vec();
        v.push(IconImage {
            width: b[e],
            height: b[e + 1],
            colors: b[e + 2],
            planes: u16_at(b, e + 4)?,
            bit_count: u16_at(b, e + 6)?,
            data,
        });
    }
    Ok(v)
}

/// Ресурс: тип, номер, данные.
struct Res {
    kind: u32,
    id: u32,
    data: Vec<u8>,
}

fn align(v: &mut Vec<u8>, to: usize) {
    while !v.len().is_multiple_of(to) {
        v.push(0);
    }
}

fn build_coff(images: &[IconImage]) -> Vec<u8> {
    // Ресурсы: каждая картинка — RT_ICON с номером 1..N, и одна группа RT_GROUP_ICON №1.
    let mut res: Vec<Res> = Vec::new();
    let mut group = Vec::new();
    group.extend_from_slice(&0u16.to_le_bytes());
    group.extend_from_slice(&1u16.to_le_bytes());
    group.extend_from_slice(&(images.len() as u16).to_le_bytes());
    for (i, img) in images.iter().enumerate() {
        let id = (i + 1) as u16;
        group.extend_from_slice(&[img.width, img.height, img.colors, 0]);
        group.extend_from_slice(&img.planes.to_le_bytes());
        group.extend_from_slice(&img.bit_count.to_le_bytes());
        group.extend_from_slice(&(img.data.len() as u32).to_le_bytes());
        group.extend_from_slice(&id.to_le_bytes());
        res.push(Res { kind: RT_ICON, id: id as u32, data: img.data.clone() });
    }
    res.push(Res { kind: RT_GROUP_ICON, id: 1, data: group });

    // Типы по возрастанию, внутри — номера по возрастанию (так требует формат).
    let mut kinds: Vec<u32> = res.iter().map(|r| r.kind).collect();
    kinds.sort();
    kinds.dedup();

    // Размеры частей: корень → каталоги типов → каталоги номеров (язык) → записи данных.
    let dir_size = |entries: usize| 16 + 8 * entries;
    let root_size = dir_size(kinds.len());
    let mut type_dirs_size = 0;
    for k in &kinds {
        type_dirs_size += dir_size(res.iter().filter(|r| r.kind == *k).count());
    }
    let lang_dirs_size = res.len() * dir_size(1);
    let data_entries_off = root_size + type_dirs_size + lang_dirs_size;
    let data_start = data_entries_off + res.len() * 16;

    let mut sec: Vec<u8> = Vec::new();
    let dir_header = |v: &mut Vec<u8>, ids: usize| {
        v.extend_from_slice(&[0u8; 12]); // характеристики, время, версия
        v.extend_from_slice(&0u16.to_le_bytes()); // именованных записей
        v.extend_from_slice(&(ids as u16).to_le_bytes()); // записей с номерами
    };
    const SUBDIR: u32 = 0x8000_0000;

    // Порядок ресурсов в таблице: по типу, затем по номеру.
    let mut order: Vec<usize> = (0..res.len()).collect();
    order.sort_by_key(|&i| (res[i].kind, res[i].id));

    // Корень.
    dir_header(&mut sec, kinds.len());
    let mut type_off = root_size;
    for k in &kinds {
        sec.extend_from_slice(&k.to_le_bytes());
        sec.extend_from_slice(&(SUBDIR | type_off as u32).to_le_bytes());
        type_off += dir_size(res.iter().filter(|r| r.kind == *k).count());
    }
    // Каталоги типов.
    let mut lang_off = root_size + type_dirs_size;
    for k in &kinds {
        let items: Vec<usize> = order.iter().copied().filter(|&i| res[i].kind == *k).collect();
        dir_header(&mut sec, items.len());
        for &i in &items {
            sec.extend_from_slice(&res[i].id.to_le_bytes());
            sec.extend_from_slice(&(SUBDIR | lang_off as u32).to_le_bytes());
            lang_off += dir_size(1);
        }
    }
    // Каталоги языков (одна запись: язык нейтральный, 0) → запись данных.
    for (n, _) in order.iter().enumerate() {
        dir_header(&mut sec, 1);
        sec.extend_from_slice(&0u32.to_le_bytes());
        sec.extend_from_slice(&((data_entries_off + n * 16) as u32).to_le_bytes());
    }
    // Записи данных; адрес данных — относительно начала секции, его поправит компоновщик (релокация).
    let mut relocs: Vec<u32> = Vec::new();
    let mut data_off = data_start;
    let mut blob: Vec<u8> = Vec::new();
    for &i in &order {
        relocs.push(sec.len() as u32);
        sec.extend_from_slice(&(data_off as u32).to_le_bytes());
        sec.extend_from_slice(&(res[i].data.len() as u32).to_le_bytes());
        sec.extend_from_slice(&0u32.to_le_bytes()); // кодовая страница
        sec.extend_from_slice(&0u32.to_le_bytes());
        blob.extend_from_slice(&res[i].data);
        align(&mut blob, 8);
        data_off = data_start + blob.len();
    }
    assert_eq!(sec.len(), data_start);
    sec.extend_from_slice(&blob);

    // --- COFF: заголовок, одна секция .rsrc, релокации, символ секции, таблица строк ---
    let header_size = 20;
    let section_header_size = 40;
    let raw_off = header_size + section_header_size;
    let reloc_off = raw_off + sec.len();
    let sym_off = reloc_off + relocs.len() * 10;

    let mut o: Vec<u8> = Vec::new();
    o.extend_from_slice(&0x8664u16.to_le_bytes()); // x86-64
    o.extend_from_slice(&1u16.to_le_bytes()); // секций
    o.extend_from_slice(&0u32.to_le_bytes()); // время
    o.extend_from_slice(&(sym_off as u32).to_le_bytes());
    o.extend_from_slice(&1u32.to_le_bytes()); // символов
    o.extend_from_slice(&0u16.to_le_bytes()); // необязательный заголовок
    o.extend_from_slice(&0u16.to_le_bytes()); // характеристики

    o.extend_from_slice(b".rsrc\0\0\0");
    o.extend_from_slice(&0u32.to_le_bytes()); // VirtualSize
    o.extend_from_slice(&0u32.to_le_bytes()); // VirtualAddress
    o.extend_from_slice(&(sec.len() as u32).to_le_bytes());
    o.extend_from_slice(&(raw_off as u32).to_le_bytes());
    o.extend_from_slice(&(reloc_off as u32).to_le_bytes());
    o.extend_from_slice(&0u32.to_le_bytes()); // номера строк
    o.extend_from_slice(&(relocs.len() as u16).to_le_bytes());
    o.extend_from_slice(&0u16.to_le_bytes());
    // Инициализированные данные, только чтение, выравнивание 4 байта.
    o.extend_from_slice(&(0x0000_0040u32 | 0x4000_0000 | 0x0030_0000).to_le_bytes());

    o.extend_from_slice(&sec);
    for r in &relocs {
        o.extend_from_slice(&r.to_le_bytes());
        o.extend_from_slice(&0u32.to_le_bytes()); // символ №0 — начало секции
        o.extend_from_slice(&3u16.to_le_bytes()); // IMAGE_REL_AMD64_ADDR32NB (адрес относительно образа)
    }
    // Символ «.rsrc»: секция 1, класс STATIC, одна дополнительная запись не нужна.
    o.extend_from_slice(b".rsrc\0\0\0");
    o.extend_from_slice(&0u32.to_le_bytes()); // значение
    o.extend_from_slice(&1i16.to_le_bytes()); // номер секции
    o.extend_from_slice(&0u16.to_le_bytes()); // тип
    o.push(3); // IMAGE_SYM_CLASS_STATIC
    o.push(0); // доп. записей
    o.extend_from_slice(&4u32.to_le_bytes()); // пустая таблица строк
    o
}
