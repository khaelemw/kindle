//! Event script files (`event/<map>.emevd.dcx`), read and written by Kindle (its own reading of
//! Dark Souls III's files, 2026-10-06; Elden Ring's are laid out the same, their flags' last byte
//! 0xFF, 2026-10-30) so mods can change the game's events.
//!
//! Layout (little endian, offsets from the file's start): "EVD\0", 4 flag bytes (00 FF 01 00),
//! +0x08 u32 version 0xCD, +0x0C u32 file size, then (count, offset) i64 pairs: +0x10 events,
//! +0x20 instructions, +0x30 an always-empty section, +0x40 layers, +0x50 parameters, +0x60
//! linked files; +0x70 the arguments' length and offset, +0x80 the strings' length and offset.
//! The sections follow in that order from +0x90: events, instructions, layers, arguments,
//! parameters, linked files, strings.
//! - Event, 0x30 bytes: i64 id, i64 instruction count, i64 offset of its first instruction (in
//!   the instruction section), i64 parameter count, i64 parameter offset (-1 without any), u32
//!   what happens when the map reloads (0 restart, 1 end, 2 end and restart), 4 zero bytes.
//! - Instruction, 0x20 bytes: u32 bank, u32 index, i64 arguments length, i64 arguments offset
//!   (in the arguments block; 0xFFFFFFFF without arguments), i64 layer offset (-1 for none).
//!   Arguments are packed in instruction order, each from a multiple of 4; the block ends padded
//!   to 16.
//! - Layer, 0x20 bytes; parameter, 0x20 bytes: i64 instruction (in its event), i64 target byte
//!   (in that instruction's arguments), i64 source byte (in the event's call arguments), i64
//!   length. Linked files are i64 offsets into the strings (UTF-16).
//!
//! Layers, linked files and strings are kept as they are.

use std::fmt;

const MAGIC: &[u8; 4] = b"EVD\0";
const HEADER: usize = 0x90;
const EVENT: usize = 0x30;
const INSTRUCTION: usize = 0x20;
const PARAMETER: usize = 0x20;
const LAYER: usize = 0x20;
const NO_ARGS: i64 = 0xFFFF_FFFF;

#[derive(Debug, Clone, PartialEq)]
pub struct Emevd {
    /// The 4 bytes after the magic and the version.
    flags: [u8; 4],
    version: u32,
    pub events: Vec<Event>,
    layers: Vec<u8>,
    linked: Vec<i64>,
    strings: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub id: i64,
    /// What happens when the map reloads: 0 restart, 1 end, 2 end and restart.
    pub rest: u32,
    pub instructions: Vec<Instruction>,
    pub parameters: Vec<Parameter>,
}

#[derive(Clone, PartialEq)]
pub struct Instruction {
    pub bank: u32,
    pub index: u32,
    pub args: Vec<u8>,
    /// Offset of its layer entry in the layer section, or -1.
    pub layer: i64,
}

impl fmt::Debug for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}[{:02}] {:02x?}", self.bank, self.index, self.args)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Parameter {
    /// The instruction (in its event) whose arguments it fills.
    pub instruction: i64,
    pub target: i64,
    pub source: i64,
    pub length: i64,
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn bytes(&self, at: usize, n: usize) -> Result<&[u8], String> {
        self.0.get(at..at + n).ok_or_else(|| format!("the event file ends before {:#x}", at + n))
    }
    fn i64(&self, at: usize) -> Result<i64, String> {
        Ok(i64::from_le_bytes(self.bytes(at, 8)?.try_into().unwrap_or_default()))
    }
    fn u32(&self, at: usize) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.bytes(at, 4)?.try_into().unwrap_or_default()))
    }
    /// A section's (count, offset) pair as usizes.
    fn section(&self, at: usize) -> Result<(usize, usize), String> {
        let (n, o) = (self.i64(at)?, self.i64(at + 8)?);
        if n < 0 || o < 0 {
            return Err(format!("a bad section at {at:#x}"));
        }
        Ok((n as usize, o as usize))
    }
}

impl Emevd {
    /// Reads an event file (already out of its DCX).
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        let r = Reader(data);
        if r.bytes(0, 4)? != MAGIC {
            return Err("not an event file".into());
        }
        let flags: [u8; 4] = r.bytes(4, 4)?.try_into().unwrap_or_default();
        let version = r.u32(8)?;
        let (event_n, event_o) = r.section(0x10)?;
        let (_, instr_o) = r.section(0x20)?;
        let (layer_n, layer_o) = r.section(0x40)?;
        let (_, param_o) = r.section(0x50)?;
        let (linked_n, linked_o) = r.section(0x60)?;
        let (_, args_o) = r.section(0x70)?;
        let (strings_n, strings_o) = r.section(0x80)?;
        let mut events = Vec::with_capacity(event_n);
        for e in 0..event_n {
            let at = event_o + e * EVENT;
            let (id, icount, ioff, pcount, poff) = (r.i64(at)?, r.i64(at + 8)?, r.i64(at + 16)?, r.i64(at + 24)?, r.i64(at + 32)?);
            let rest = r.u32(at + 40)?;
            let mut instructions = Vec::with_capacity(icount.max(0) as usize);
            for k in 0..icount.max(0) as usize {
                let at = instr_o + ioff as usize + k * INSTRUCTION;
                let (bank, index) = (r.u32(at)?, r.u32(at + 4)?);
                let (alen, aoff, layer) = (r.i64(at + 8)?, r.i64(at + 16)?, r.i64(at + 24)?);
                let args = if alen > 0 { r.bytes(args_o + aoff as usize, alen as usize)?.to_vec() } else { Vec::new() };
                instructions.push(Instruction { bank, index, args, layer });
            }
            let mut parameters = Vec::with_capacity(pcount.max(0) as usize);
            for k in 0..pcount.max(0) as usize {
                let at = param_o + poff as usize + k * PARAMETER;
                parameters.push(Parameter { instruction: r.i64(at)?, target: r.i64(at + 8)?, source: r.i64(at + 16)?, length: r.i64(at + 24)? });
            }
            events.push(Event { id, rest, instructions, parameters });
        }
        let linked = (0..linked_n).map(|k| r.i64(linked_o + 8 * k)).collect::<Result<_, _>>()?;
        Ok(Self { flags, version, events, layers: r.bytes(layer_o, layer_n * LAYER)?.to_vec(), linked, strings: r.bytes(strings_o, strings_n)?.to_vec() })
    }

    /// Writes the file (to be put in a DCX).
    pub fn to_bytes(&self) -> Vec<u8> {
        let instr_n: usize = self.events.iter().map(|e| e.instructions.len()).sum();
        let param_n: usize = self.events.iter().map(|e| e.parameters.len()).sum();
        let event_o = HEADER;
        let instr_o = event_o + self.events.len() * EVENT;
        let empty_o = instr_o + instr_n * INSTRUCTION;
        let layer_o = empty_o;
        let args_o = layer_o + self.layers.len();
        // The arguments, packed in instruction order from multiples of 4.
        let mut args = Vec::new();
        let mut args_at = Vec::with_capacity(instr_n);
        for i in self.events.iter().flat_map(|e| &e.instructions) {
            if i.args.is_empty() {
                args_at.push(NO_ARGS);
                continue;
            }
            args.resize(args.len().div_ceil(4) * 4, 0);
            args_at.push(args.len() as i64);
            args.extend_from_slice(&i.args);
        }
        args.resize(args.len().div_ceil(16) * 16, 0);
        let param_o = args_o + args.len();
        let linked_o = param_o + param_n * PARAMETER;
        let strings_o = linked_o + self.linked.len() * 8;
        let size = strings_o + self.strings.len();

        let mut out = Vec::with_capacity(size);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&self.flags);
        out.extend_from_slice(&self.version.to_le_bytes());
        out.extend_from_slice(&(size as u32).to_le_bytes());
        for (n, o) in [
            (self.events.len(), event_o),
            (instr_n, instr_o),
            (0, empty_o),
            (self.layers.len() / LAYER, layer_o),
            (param_n, param_o),
            (self.linked.len(), linked_o),
            (args.len(), args_o),
            (self.strings.len(), strings_o),
        ] {
            out.extend_from_slice(&(n as i64).to_le_bytes());
            out.extend_from_slice(&(o as i64).to_le_bytes());
        }
        out.resize(HEADER, 0);
        let (mut ins, mut par) = (0usize, 0usize);
        for e in &self.events {
            out.extend_from_slice(&e.id.to_le_bytes());
            out.extend_from_slice(&(e.instructions.len() as i64).to_le_bytes());
            out.extend_from_slice(&((ins * INSTRUCTION) as i64).to_le_bytes());
            out.extend_from_slice(&(e.parameters.len() as i64).to_le_bytes());
            let poff = if e.parameters.is_empty() { -1 } else { (par * PARAMETER) as i64 };
            out.extend_from_slice(&poff.to_le_bytes());
            out.extend_from_slice(&e.rest.to_le_bytes());
            out.extend_from_slice(&[0; 4]);
            ins += e.instructions.len();
            par += e.parameters.len();
        }
        for (i, at) in self.events.iter().flat_map(|e| &e.instructions).zip(&args_at) {
            out.extend_from_slice(&i.bank.to_le_bytes());
            out.extend_from_slice(&i.index.to_le_bytes());
            out.extend_from_slice(&(i.args.len() as i64).to_le_bytes());
            out.extend_from_slice(&at.to_le_bytes());
            out.extend_from_slice(&i.layer.to_le_bytes());
        }
        out.extend_from_slice(&self.layers);
        out.extend_from_slice(&args);
        for p in self.events.iter().flat_map(|e| &e.parameters) {
            for v in [p.instruction, p.target, p.source, p.length] {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        for l in &self.linked {
            out.extend_from_slice(&l.to_le_bytes());
        }
        out.extend_from_slice(&self.strings);
        out
    }

    pub fn event(&self, id: i64) -> Option<&Event> {
        self.events.iter().find(|e| e.id == id)
    }

    pub fn event_mut(&mut self, id: i64) -> Option<&mut Event> {
        self.events.iter_mut().find(|e| e.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Emevd {
        Emevd {
            flags: [0, 0xFF, 1, 0],
            version: 0xCD,
            events: vec![
                Event {
                    id: 0,
                    rest: 0,
                    instructions: vec![
                        Instruction { bank: 2000, index: 0, args: vec![1, 0, 0, 0, 0x10, 0x27, 0, 0], layer: -1 },
                        Instruction { bank: 1014, index: 0, args: vec![], layer: -1 },
                        Instruction { bank: 2003, index: 4, args: vec![5, 0, 0], layer: 0 },
                    ],
                    parameters: vec![],
                },
                Event {
                    id: 20005110,
                    rest: 1,
                    instructions: vec![Instruction { bank: 2003, index: 66, args: vec![0; 12], layer: -1 }],
                    parameters: vec![Parameter { instruction: 0, target: 4, source: 0, length: 4 }],
                },
            ],
            layers: vec![2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 1, 0, 0, 0, 0, 0, 0, 0],
            linked: vec![0],
            strings: "a\0".encode_utf16().flat_map(|c| c.to_le_bytes()).collect(),
        }
    }

    #[test]
    fn event_files_round_trip() {
        let e = sample();
        let bytes = e.to_bytes();
        assert_eq!(&bytes[0..4], MAGIC);
        assert_eq!(u32::from_le_bytes(bytes[0xC..0x10].try_into().unwrap()) as usize, bytes.len());
        let back = Emevd::parse(&bytes).unwrap();
        assert_eq!(back, e);
        assert_eq!(back.to_bytes(), bytes);
        // No arguments: the 0xFFFFFFFF offset; the next ones start at a multiple of 4.
        let instr_o = HEADER + 2 * EVENT;
        assert_eq!(i64::from_le_bytes(bytes[instr_o + INSTRUCTION + 16..instr_o + INSTRUCTION + 24].try_into().unwrap()), NO_ARGS);
        assert_eq!(i64::from_le_bytes(bytes[instr_o + 2 * INSTRUCTION + 16..instr_o + 2 * INSTRUCTION + 24].try_into().unwrap()), 8);
    }
}

#[cfg(test)]
pub(crate) mod tests_support {
    /// An event file with no events.
    pub fn empty() -> Vec<u8> {
        super::Emevd { flags: [0, 0xFF, 1, 0], version: 0xCD, events: vec![], layers: vec![], linked: vec![], strings: vec![] }.to_bytes()
    }
}
