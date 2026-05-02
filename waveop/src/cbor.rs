#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct InputFile {
    #[serde(rename = "U")]
    pub u: u64,
    #[serde(rename = "l")]
    pub l: u64,
    #[serde(rename = "m")]
    pub m: u64,
    #[serde(rename = "s")]
    pub s: u64,
    #[serde(rename = "Rs")]
    pub rs: u64,
    #[serde(rename = "values")]
    pub values: Vec<i64>,
}

impl InputFile {
    pub fn new(u: u64, l: u64, m: u64, s: u64, rs: u64, values: Vec<i64>) -> anyhow::Result<Self> {
        let me = Self { u, l, m, s, rs, values };
        me.validate()?;
        Ok(me)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.values.len() as u64 != self.u * (1 << self.l) {
            anyhow::bail!("values length ({}) does not match U * 2^l ({})", self.values.len(), self.u * (1 << self.l));
        }
        Ok(())
    }

    pub fn to_writer<W: Write>(&self, mut w: W) -> anyhow::Result<()> {
        self.validate()?;
        serde_cbor::to_writer(&mut w, self)?;
        Ok(())
    }

    pub fn to_vec(&self) -> anyhow::Result<Vec<u8>> {
        self.validate()?;
        Ok(serde_cbor::to_vec(self)?)
    }

    pub fn write_file<P: AsRef<std::path::Path>>(&self, path: P) -> anyhow::Result<()> {
        let f = File::create(path)?;
        let mut bw = BufWriter::new(f);
        self.to_writer(&mut bw)?;
        bw.flush()?;
        Ok(())
    }

    pub fn from_reader<R: Read>(r: R) -> anyhow::Result<Self> {
        let me: Self = serde_cbor::from_reader(r)?;
        me.validate()?;
        Ok(me)
    }

    pub fn read_file<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<Self> {
        let f = File::open(path)?;
        let br = BufReader::new(f);
        Self::from_reader(br)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct OutputFile {
    #[serde(rename = "values")]
    pub values: Vec<i64>,
}

impl OutputFile {
    pub fn new(values: Vec<i64>) -> Self {
        Self { values }
    }

    pub fn to_writer<W: Write>(&self, mut w: W) -> anyhow::Result<()> {
        serde_cbor::to_writer(&mut w, self)?;
        Ok(())
    }

    pub fn to_vec(&self) -> anyhow::Result<Vec<u8>> {
        Ok(serde_cbor::to_vec(self)?)
    }

    pub fn write_file<P: AsRef<std::path::Path>>(&self, path: P) -> anyhow::Result<()> {
        let f = File::create(path)?;
        let mut bw = BufWriter::new(f);
        self.to_writer(&mut bw)?;
        bw.flush()?;
        Ok(())
    }

    pub fn from_reader<R: Read>(r: R) -> anyhow::Result<Self> {
        let me: Self = serde_cbor::from_reader(r)?;
        Ok(me)
    }

    pub fn read_file<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<Self> {
        let f = File::open(path)?;
        let br = BufReader::new(f);
        Self::from_reader(br)
    }
}
