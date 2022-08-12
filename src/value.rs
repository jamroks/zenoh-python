use pyo3::{prelude::*, types::PyBytes};
use uhlc::Timestamp;
use zenoh::{
    prelude::{Encoding, KeyExpr, Sample, Value, ZenohId},
    query::Reply,
    scouting::Hello,
};
use zenoh_buffers::{SplitBuffer, ZBuf};

use crate::{
    enums::{_Encoding, _SampleKind},
    keyexpr::_KeyExpr,
    ToPyErr,
};

#[derive(Clone)]
pub(crate) enum Payload {
    Zenoh(ZBuf),
    Python(Py<PyBytes>),
}
impl Payload {
    pub(crate) fn into_zbuf(self) -> ZBuf {
        match self {
            Payload::Zenoh(buf) => buf,
            Payload::Python(buf) => Python::with_gil(|py| ZBuf::from(buf.as_bytes(py).to_owned())),
        }
    }
    pub(crate) fn into_pybytes(self) -> Py<PyBytes> {
        match self {
            Payload::Zenoh(buf) => {
                Python::with_gil(|py| Py::from(PyBytes::new(py, buf.contiguous().as_ref())))
            }
            Payload::Python(buf) => buf,
        }
    }
}
impl From<ZBuf> for Payload {
    fn from(buf: ZBuf) -> Self {
        Payload::Zenoh(buf)
    }
}
impl From<Py<PyBytes>> for Payload {
    fn from(buf: Py<PyBytes>) -> Self {
        Payload::Python(buf)
    }
}
#[pyclass(subclass)]
#[derive(Clone)]
pub struct _Value {
    pub(crate) payload: Payload,
    pub(crate) encoding: Encoding,
}
#[pymethods]
impl _Value {
    #[new]
    pub fn pynew(this: Self) -> Self {
        this
    }
    #[staticmethod]
    pub fn new(payload: Py<PyBytes>, encoding: Option<_Encoding>) -> Self {
        Self {
            payload: payload.into(),
            encoding: encoding.map(|e| e.0).unwrap_or(Encoding::EMPTY),
        }
    }
    #[getter]
    pub fn payload(&mut self) -> Py<PyBytes> {
        if let Payload::Python(buf) = &self.payload {
            return buf.clone();
        }
        let payload = unsafe { std::ptr::read(&self.payload) };
        let buf = payload.into_pybytes();
        unsafe { std::ptr::write(&mut self.payload, Payload::Python(buf.clone())) };
        buf
    }
    pub fn with_payload(&mut self, payload: Py<PyBytes>) {
        self.payload = Payload::Python(payload)
    }
    #[getter]
    pub fn encoding(&self) -> _Encoding {
        _Encoding(self.encoding.clone())
    }
    pub fn with_encoding(&mut self, encoding: _Encoding) {
        self.encoding = encoding.0;
    }
}
impl From<Value> for _Value {
    fn from(value: Value) -> Self {
        _Value {
            payload: value.payload.into(),
            encoding: value.encoding,
        }
    }
}
impl From<_Value> for Value {
    fn from(value: _Value) -> Self {
        Value::new(value.payload.into_zbuf()).encoding(value.encoding)
    }
}

pub(crate) trait PyAnyToValue {
    fn to_value(self) -> PyResult<Value>;
}
impl PyAnyToValue for &PyAny {
    fn to_value(self) -> PyResult<Value> {
        let encoding: _Encoding = self.getattr("encoding")?.extract()?;
        let payload: &PyBytes = self.getattr("payload")?.extract()?;
        Ok(Value::new(ZBuf::from(payload.as_bytes().to_owned())).encoding(encoding.0))
    }
}

#[pyclass(subclass)]
#[derive(Clone)]
pub struct _Sample {
    key_expr: KeyExpr<'static>,
    value: _Value,
    kind: _SampleKind,
    timestamp: Option<_Timestamp>,
}
impl From<Sample> for _Sample {
    fn from(sample: Sample) -> Self {
        let Sample {
            key_expr,
            value,
            kind,
            timestamp,
            ..
        } = sample;
        _Sample {
            key_expr,
            value: value.into(),
            kind: _SampleKind(kind),
            timestamp: timestamp.map(_Timestamp),
        }
    }
}

#[pyclass(subclass)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct _ZenohId(pub(crate) ZenohId);
#[pymethods]
impl _ZenohId {
    #[new]
    pub fn pynew(this: Self) -> Self {
        this
    }
    pub fn __str__(&self) -> String {
        self.0.to_string()
    }
}

#[pyclass(subclass)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct _Timestamp(Timestamp);
#[pymethods]
impl _Timestamp {
    #[new]
    pub fn pynew(this: Self) -> Self {
        this
    }
    crate::derive_richcmp!();
    #[getter]
    pub fn seconds_since_unix_epoch(&self) -> PyResult<f64> {
        match self
            .0
            .get_time()
            .to_system_time()
            .duration_since(std::time::UNIX_EPOCH)
        {
            Ok(o) => Ok(o.as_secs_f64()),
            Err(e) => Err(e.to_pyerr()),
        }
    }
}

#[pymethods]
impl _Sample {
    #[new]
    pub fn pynew(this: Self) -> Self {
        this
    }
    #[getter]
    pub fn value(&self) -> _Value {
        self.value.clone()
    }
    #[getter]
    pub fn key_expr(&self) -> _KeyExpr {
        _KeyExpr(self.key_expr.clone())
    }
    #[getter]
    pub fn payload(&mut self) -> Py<PyBytes> {
        if let Payload::Python(buf) = &self.value.payload {
            return buf.clone();
        }
        let payload = unsafe { std::ptr::read(&self.value.payload) };
        let buf = payload.into_pybytes();
        unsafe { std::ptr::write(&mut self.value.payload, Payload::Python(buf.clone())) };
        buf
    }
    #[getter]
    pub fn encoding(&self) -> _Encoding {
        _Encoding(self.value.encoding.clone())
    }
    #[getter]
    pub fn kind(&self) -> _SampleKind {
        self.kind.clone()
    }
    #[getter]
    pub fn timestamp(&self) -> Option<_Timestamp> {
        self.timestamp
    }
    #[staticmethod]
    pub fn new(
        key_expr: _KeyExpr,
        value: _Value,
        kind: _SampleKind,
        timestamp: Option<_Timestamp>,
    ) -> Self {
        _Sample {
            key_expr: key_expr.0,
            value,
            kind,
            timestamp,
        }
    }
}

impl From<_Sample> for Sample {
    fn from(sample: _Sample) -> Self {
        let _Sample {
            key_expr,
            value,
            kind,
            timestamp,
        } = sample;
        let mut sample = Sample::new(key_expr, value);
        sample.kind = kind.0;
        sample.timestamp = timestamp.map(|t| t.0);
        sample
    }
}

#[pyclass(subclass)]
#[derive(Clone)]
pub struct _Reply {
    #[pyo3(get)]
    pub replier_id: _ZenohId,
    pub reply: Result<_Sample, _Value>,
}
#[pymethods]
impl _Reply {
    #[new]
    pub fn pynew(this: Self) -> Self {
        this
    }
    #[getter]
    pub fn ok(&self) -> PyResult<_Sample> {
        match &self.reply {
            Ok(o) => Ok(o.clone()),
            Err(_) => Err(zenoh_core::zerror!("Called `Reply.ok` on a non-ok reply.").to_pyerr()),
        }
    }
    #[getter]
    pub fn err(&self) -> PyResult<_Value> {
        match &self.reply {
            Err(o) => Ok(o.clone()),
            Ok(_) => Err(zenoh_core::zerror!("Called `Reply.err` on a non-err reply.").to_pyerr()),
        }
    }
}
impl From<Reply> for _Reply {
    fn from(reply: Reply) -> Self {
        _Reply {
            replier_id: _ZenohId(reply.replier_id),
            reply: match reply.sample {
                Ok(o) => Ok(o.into()),
                Err(e) => Err(e.into()),
            },
        }
    }
}

#[pyclass(subclass)]
#[derive(Clone)]
pub struct _Hello(pub(crate) Hello);
#[pymethods]
impl _Hello {
    #[new]
    pub fn pynew(this: Self) -> Self {
        this
    }
    #[getter]
    pub fn zid(&self) -> Option<_ZenohId> {
        self.0.zid.map(_ZenohId)
    }
    #[getter]
    pub fn whatami(&self) -> Option<&'static str> {
        match self.0.whatami {
            Some(zenoh::config::whatami::WhatAmI::Client) => Some("client"),
            Some(zenoh::config::whatami::WhatAmI::Peer) => Some("peer"),
            Some(zenoh::config::whatami::WhatAmI::Router) => Some("router"),
            None => None,
        }
    }
    #[getter]
    pub fn locators(&self) -> Vec<String> {
        match &self.0.locators {
            Some(locators) => locators.iter().map(|l| l.to_string()).collect(),
            None => Vec::new(),
        }
    }
    pub fn __str__(&self) -> String {
        self.0.to_string()
    }
}
impl From<Hello> for _Hello {
    fn from(h: Hello) -> Self {
        _Hello(h)
    }
}