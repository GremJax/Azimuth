use std::{collections::{HashMap, HashSet}, fs};
use ordered_float::OrderedFloat;

use crate::{
    analyzer::{AzimuthInfo, LocalId, ObjectInfo, ResolvedExpression, ResolvedFunctionBody, ResolvedStatement, ShapeInfo, Symbol}, 
    executor::{RuntimeError, ShapeInstance}, lexer::Span, loader::Loader, parser::MappingKind,
};

pub mod lexer;
pub mod parser;
pub mod loader;
pub mod analyzer;
pub mod executor;
pub mod intrinsic;

// Formerly SlotBox

// Runtime Value
#[derive(Default, Debug, Clone, Hash, PartialEq, Eq)]
pub enum ValueKind {
    Number(NumKind),
    Bool,
    String,
    Range(NumKind),
    Shape(ShapeInstance),
    Array(Box<ValueKind>),
    Set(Box<ValueKind>),
    Dict(Box<ValueKind>, Box<ValueKind>),
    Azimuth(Box<ValueKind>),
    Option(Box<ValueKind>),
    Object(Vec<ValueKind>),
    Local(Box<ValueKind>),
    Pointer(Box<ValueKind>),
    ArrayElement(Box<ValueKind>),
    Function(Box<FunctionSignature>),
    Generic(GenericId, Vec<ValueKind>),
    Multiple(Vec<ValueKind>),
    #[default] None
}

impl ValueKind {
    fn is_assignable_from_runtime(&self, other_raw:ValueKind, runtime:&Runtime) -> bool {
        if self.is_assignable_from(other_raw.clone()) { return true }

        match self {
            ValueKind::Shape(inst) => {
                let shape = runtime.get_shape(inst.id);
                match shape {
                    Some(info) => {
                        for parent in &info.parent_ids {
                            let parent_inst = ShapeInstance{id:parent.id, generics:inst.generics.clone()};
                            println!("Checking shape {:?} against {:?}", parent_inst, other_raw);
                            if ValueKind::Shape(parent_inst.clone()).is_assignable_from(other_raw.clone()) { return true }
                        }
                        false
                    }
                    None => false
                }
            }
            _ => false
        }
    }

    fn is_assignable_from(&self, other_raw: ValueKind) -> bool {
        let other = match other_raw {
            ValueKind::Option(kind) => {
                if matches!(self, ValueKind::None) { return true }
                *kind
            }
            ValueKind::Object(shapes) => {
                for shape in shapes {
                    if !self.is_assignable_from(shape) { return false }
                }
                return true
            }
            ValueKind::Generic(_, shapes) => {
                for shape in shapes {
                    if !self.is_assignable_from(shape) { return false }
                }
                return true
            }
            kind => kind
        };
        match self {
            ValueKind::Number(_) => other == ValueKind::Number(NumKind::Any) || other.is_assignable_from(ValueKind::Number(NumKind::Any)),

            ValueKind::Bool => other == ValueKind::Bool,
            ValueKind::String => other == ValueKind::String,
            
            ValueKind::Shape(k) => other == ValueKind::Shape(k.clone()),
            ValueKind::Array(k) => match other {
                ValueKind::Array(other_k) => k.is_assignable_from(*other_k),
                _ => false,
            },
            ValueKind::Set(k) => match other {
                ValueKind::Set(other_k) => k.is_assignable_from(*other_k),
                _ => false,
            },
            ValueKind::Dict(k, v) => match other {
                ValueKind::Dict(other_k, other_v) => k.is_assignable_from(*other_k) && v.is_assignable_from(*other_v),
                _ => false,
            },
            ValueKind::Range(k) => match other {
                ValueKind::Range(other_k) => ValueKind::Number(*k).is_assignable_from(ValueKind::Number(other_k)),
                //ValueKind::Array(other_k) => ValueKind::Number(*k).is_assignable_from(*other_k),
                _ => false,
            },
            ValueKind::Azimuth(k) => match other {
                ValueKind::Azimuth(other_k) => k.is_assignable_from(*other_k),
                _ => false,
            },
            ValueKind::Object(k) => {
                for shape in k {
                    if shape.is_assignable_from(other.clone()) { return true }
                }
                false
            }
            ValueKind::Generic(_, k) => {
                for shape in k {
                    if shape.is_assignable_from(other.clone()) { return true }
                }
                false
            }
            ValueKind::Local(k) => k.is_assignable_from(other),
            ValueKind::Pointer(k) => k.is_assignable_from(other),
            ValueKind::ArrayElement(k) => k.is_assignable_from(other),

            ValueKind::Function(info) => {
                // Actions that return values are ok
                if info.input_types.is_empty() && info.output_type.is_assignable_from(other.clone()) {
                    return true
                }

                match other {
                    ValueKind::Function(other) => {
                        if !info.output_type.is_assignable_from(other.output_type) {
                            return false
                        }
                        for i in 0..other.input_types.len() {
                            if !info.input_types[i].is_assignable_from(other.input_types[i].clone()) {
                                return false
                            }
                        }
                        true
                    },
                    _ => false
                }
            }

            ValueKind::Option(k) => k.is_assignable_from(other),
            ValueKind::Generic(_,_) => true,
            ValueKind::Multiple(kinds) => kinds.iter().any(|kind| kind.is_assignable_from(other.clone())),

            ValueKind::None => other == ValueKind::None,
        }
    }

}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum Number {
    Int8(i8),
    Int16(i16),
    Int32(i32),
    Int64(i64),
    UInt8(u8),
    UInt16(u16),
    UInt32(u32),
    UInt64(u64),
    Float32(OrderedFloat<f32>),
    Float64(OrderedFloat<f64>),
    Any(OrderedFloat<f64>),
}

#[derive(Copy, Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum NumKind {
    Any,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    Int8,
    Int16,
    Int32,
    Int64,
    Float32,
    Float64,
}

impl Number {
    fn num_kind(&self) -> NumKind {
        match self {
            Number::UInt8(_) => NumKind::UInt8,
            Number::UInt16(_) => NumKind::UInt16,
            Number::UInt32(_) => NumKind::UInt32,
            Number::UInt64(_) => NumKind::UInt64,
            Number::Int8(_) => NumKind::Int8,
            Number::Int16(_) => NumKind::Int16,
            Number::Int32(_) => NumKind::Int32,
            Number::Int64(_) => NumKind::Int64,
            Number::Float32(_) => NumKind::Float32,
            Number::Float64(_) => NumKind::Float64,
            Number::Any(_) => NumKind::Any,
            _ => todo!()
        }
    }

    fn to_u8(&self) -> u8 {
        match self {
            Number::UInt8(val) => *val,
            num => panic!("{:?} not convertible to uint8", num),
        }
    }

    fn to_i8(&self) -> i8 {
        match self {
            Number::Int8(val) => *val,
            num => panic!("{:?} not convertible to int8", num),
        }
    }

    fn to_u16(&self) -> u16 {
        match self {
            Number::UInt16(val) => *val,
            Number::UInt8(val) => (*val).into(),
            num => panic!("{:?} not convertible to uint16", num),
        }
    }

    fn to_i16(&self) -> i16 {
        match self {
            Number::Int16(val) => *val,
            Number::UInt8(val) => (*val).into(),
            Number::Int8(val) => (*val).into(),
            num => panic!("{:?} not convertible to int16", num),
        }
    }

    fn to_u32(&self) -> u32 {
        match self {
            Number::UInt32(val) => *val,
            Number::UInt16(val) => (*val).into(),
            Number::UInt8(val) => (*val).into(),
            num => panic!("{:?} not convertible to uint32", num),
        }
    }

    fn to_i32(&self) -> i32 {
        match self {
            Number::Int32(val) => *val,
            Number::UInt16(val) => (*val).into(),
            Number::Int16(val) => (*val).into(),
            Number::UInt8(val) => (*val).into(),
            Number::Int8(val) => (*val).into(),
            num => panic!("{:?} not convertible to int32", num),
        }
    }

    fn to_u64(&self) -> u64 {
        match self {
            Number::UInt64(val) => *val,
            Number::UInt32(val) => (*val).into(),
            Number::UInt16(val) => (*val).into(),
            Number::UInt8(val) => (*val).into(),
            num => panic!("{:?} not convertible to uint64", num),
        }
    }

    fn to_i64(&self) -> i64 {
        match self {
            Number::Int64(val) => *val,
            Number::UInt32(val) => (*val).into(),
            Number::Int32(val) => (*val).into(),
            Number::UInt16(val) => (*val).into(),
            Number::Int16(val) => (*val).into(),
            Number::UInt8(val) => (*val).into(),
            Number::Int8(val) => (*val).into(),
            num => panic!("{:?} not convertible to int64", num),
        }
    }

    fn to_f32(&self) -> f32 {
        match self {
            Number::Float32(val) => (*val).into(),
            Number::UInt16(val) => (*val).into(),
            Number::Int16(val) => (*val).into(),
            Number::UInt8(val) => (*val).into(),
            Number::Int8(val) => (*val).into(),
            num => panic!("{:?} not convertible to float32", num),
        }
    }

    fn to_f64(&self) -> f64 {
        match self {
            Number::Float64(val) => (*val).into(),
            Number::Float32(val) => (Number::Float32(*val)).to_f32().into(),
            Number::UInt32(val) => (*val).into(),
            Number::Int32(val) => (*val).into(),
            Number::UInt16(val) => (*val).into(),
            Number::Int16(val) => (*val).into(),
            Number::UInt8(val) => (*val).into(),
            Number::Int8(val) => (*val).into(),
            num => panic!("{:?} not convertible to float64", num),
        }
    }

    fn promote_kind(a: NumKind, b: NumKind) -> NumKind {
        use NumKind::*;

        match (a, b) {
            (Float64, _) | (_, Float64) => Float64,
            (Float32, _) | (_, Float32) => Float32,

            (UInt64, _) | (_, UInt64) => UInt64,
            (Int64, _) | (_, Int64) => Int64,

            (UInt32, _) | (_, UInt32) => UInt32,
            (Int32, _) | (_, Int32) => Int32,

            (UInt16, _) | (_, UInt16) => UInt16,
            (Int16, _) | (_, Int16) => Int16,

            _ => a.max(b),
        }
    }

    fn kind(&self) -> ValueKind {
        ValueKind::Number(self.num_kind())
    }

    fn to_string(&self) -> String {
        match self {
            Number::UInt8(val) => format!("{}", val),
            Number::UInt16(val) => format!("{}", val),
            Number::UInt32(val) => format!("{}", val),
            Number::UInt64(val) => format!("{}", val),
            Number::Int8(val) => format!("{}", val),
            Number::Int16(val) => format!("{}", val),
            Number::Int32(val) => format!("{}", val),
            Number::Int64(val) => format!("{}", val),
            Number::Float32(val) => format!("{}", val),
            Number::Float64(val) => format!("{}", val),
            Number::Any(val) => format!("{}", val),
        }
    }
}

type GenericId = u8;

#[derive(Default, Debug, Clone, Hash, PartialEq, Eq)]
pub enum Value {
    Number(Number),
    Bool(bool),
    String(String),

    Array(Vec<Value>, ValueKind),
    Set(Vec<Value>, ValueKind),
    Dict(Vec<(Value, Value)>, ValueKind, ValueKind),
    Range(Number, Number, Number, bool, NumKind),
    
    Azimuth(AzimuthState),
    Object(ObjectId, ValueKind),
    Local(LocalId, ValueKind),
    Pointer(ObjectId, AzimuthId, ValueKind),
    ArrayElement(ObjectId, AzimuthId, ValueKind, usize),

    Shape(ShapeInstance),

    Function(Box<Function>),
    FunctionChain(Vec<MappingTo>, ValueKind),

    #[default] None,
}

impl Value {
    fn is(&self, kind: &ValueKind) -> bool { self.kind() == *kind }

    fn to_string(&self) -> String {
        match self {
            Value::Number(val) => val.to_string(),
            Value::Bool(val) => format!("{}", val),
            Value::String(val) => val.clone(),
            Value::Array(array, _) => {
                let mut string = String::new();
                string.push('[');
                for val in array {
                    string += &val.to_string();
                    string.push(',');
                }
                string.push(']');
                string
            }
            Value::Set(array, _) => {
                let mut string = String::new();
                string.push('{');
                for val in array {
                    string += &val.to_string();
                    string.push(',');
                }
                string.push('}');
                string
            }
            Value::Dict(array, _, _) => {
                let mut string = String::new();
                string.push('{');
                if array.len() == 0 { string.push(':') }
                for (key, val) in array {
                    string += &key.to_string();
                    string.push(':');
                    string += &val.to_string();
                    string.push(',');
                }
                string.push('}');
                string
            }
            Value::Range(start, end, by, inclusive, _) =>
                format!("{}{}{}", start.to_string(), if *inclusive { "..." } else { "..<" }, end.to_string()),
            Value::Object(id, shape) =>
                format!("Obj{}: {:?}", id, shape),
            Value::Pointer(id, az, shape) =>
                format!("Obj{}.{}: {:?}", id, az, shape),
            Value::Function(func) => 
                format!("|({:?}) -> {:?} {:?}|", func.input_types, func.output_type, func.func),
            other => format!("{:?}", other),
        }
    }

    fn kind(&self) -> ValueKind {
        match self {
            Value::Number(num) => num.kind(),
            Value::Bool(_) => ValueKind::Bool,
            Value::String(_) => ValueKind::String,
            Value::Array(_, value_type) => ValueKind::Array(Box::new(value_type.clone())),
            Value::Set(_, value_type) => ValueKind::Set(Box::new(value_type.clone())),
            Value::Dict(_, key_type, value_type) => ValueKind::Dict(Box::new(key_type.clone()),Box::new(value_type.clone())),
            Value::Range(_,_,_,_,kind) => ValueKind::Range(kind.clone()),
            Value::Azimuth(s) => ValueKind::Azimuth(Box::new(s.value_type.clone())),
            Value::Object(_, kind) => kind.clone(),
            Value::Shape(inst) => ValueKind::Shape(inst.clone()),
            Value::Local(_, kind) => kind.clone(),
            Value::Pointer(_, _, kind) => kind.clone(),
            Value::ArrayElement(_, _, kind, _) => kind.clone(),
            Value::Function(func) => 
                ValueKind::Function(Box::new(FunctionSignature{
                    has_self:func.has_self,
                    output_type:func.output_type.clone(),
                    input_types:func.input_types.iter().map(|param| param.kind.clone()).collect(),
                })),
            Value::FunctionChain(_, info) => info.clone(),
            Value::None => ValueKind::None,
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Value::Bool(value)
    }
}

impl From<u8> for Value {
    fn from(value: u8) -> Self {
        Value::Number(Number::UInt8(value))
    }
}

impl From<i8> for Value {
    fn from(value: i8) -> Self {
        Value::Number(Number::Int8(value))
    }
}

impl From<u16> for Value {
    fn from(value: u16) -> Self {
        Value::Number(Number::UInt16(value))
    }
}

impl From<i16> for Value {
    fn from(value: i16) -> Self {
        Value::Number(Number::Int16(value))
    }
}

impl From<u32> for Value {
    fn from(value: u32) -> Self {
        Value::Number(Number::UInt32(value))
    }
}

impl From<i32> for Value {
    fn from(value: i32) -> Self {
        Value::Number(Number::Int32(value))
    }
}

impl From<u64> for Value {
    fn from(value: u64) -> Self {
        Value::Number(Number::UInt64(value))
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Value::Number(Number::Int64(value))
    }
}

impl From<f32> for Value {
    fn from(value: f32) -> Self {
        Value::Number(Number::Float32(value.into()))
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Value::Number(Number::Float64(value.into()))
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Value::String(value)
    }
}

// Native Function
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Function {
    pub id: AzimuthId,
    pub has_self:bool,
    pub input_types: Vec<FunctionParameter>,
    pub output_type: ValueKind,
    pub func: Option<ResolvedFunctionBody>,
    pub captures: Vec<LocalId>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct FunctionParameter {
    pub kind: ValueKind,
    pub local: Option<LocalId>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct FunctionSignature {
    pub input_types: Vec<ValueKind>,
    pub output_type: ValueKind,
    pub has_self: bool,
}

// Slot Id
pub type AzimuthId = u32;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct Azimuth {
    shape_id: ShapeId,
    id: AzimuthId,
    name: String,
    value_type: ValueKind,
    flags: AzimuthFlags,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct AzimuthFlags {
    is_static: bool,
    is_abstract: bool,
    is_locked: bool,
    pub(crate) is_const: bool,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct AzimuthState {
    azimuth: AzimuthId,
    value_type: ValueKind,
}

#[derive(Debug, Clone)]
pub struct Mapping {
    pub from: AzimuthId,
    pub to: AzimuthId,
    pub kind: MappingKind,
}

type SlotStateId = u32;

#[derive(Default, Debug, Clone)]
struct SlotState {
    sealed: bool,
    storage: Value,
}

// Shape
pub type ShapeId = u32;
pub struct Shape {
    id: ShapeId,
    name: String,
    
    num_generics: u32,
    azimuths: Vec<AzimuthId>,
    def_mappings: HashMap<AzimuthId, AzimuthId>,
    static_object_id: ObjectId,
    parent_ids: Vec<u32>,
}

impl Shape {
    fn define_slot(&mut self, id: AzimuthId, name: String, 
        kind: ValueKind, flags: AzimuthFlags) -> Azimuth {
        let slot = Azimuth {
            shape_id: self.id,
            id: id.clone(),
            name: format!("{}.{}", self.name, name),
            value_type: kind,
            flags,
        };
        self.azimuths.push(id);
        slot
    }

    fn remap_slot(&mut self, from: AzimuthId, to: AzimuthId) {
        //if !to.value_type.is_assignable_from(from.value_type.clone()) { 
        //    panic!("Type mismatch: {:?} not assignable from {:?}", to.value_type, from.value_type); 
        //}
        
        self.def_mappings.insert(from.clone(), to.clone());
    }
}

// Object
pub type ObjectId = u32;

#[derive(Debug, Clone)]
pub struct ObjectFlags {
    sealed: bool,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum MappingTo {
    Slot(SlotStateId),
    Chain(Vec<MappingTo>, usize),
    Map(AzimuthId),
    Link(ObjectId, AzimuthId),
    Expression(ResolvedExpression),
}

#[derive(Debug, Clone)]
pub struct Object {
    id: ObjectId,
    name: String,

    flags: ObjectFlags,

    slot_mapping: Vec<(AzimuthState, MappingTo)>,
    slot_states: Vec<SlotState>,
    free_slots: Vec<SlotStateId>,
    next_slot_state_id: SlotStateId,
}

impl Object {
    fn allocate_slot(&mut self) -> SlotStateId {
        if let Some(id) = self.free_slots.pop() {
            self.slot_states[id as usize] = SlotState::default();
            id
        } else {
            let id = self.next_slot_state_id;
            self.next_slot_state_id += 1;
            self.slot_states.push(SlotState::default());
            id
        }
    }

    fn free_slot(&mut self, slot_id: SlotStateId) {
        if slot_id as usize >= self.slot_states.len() {
            panic!("Invalid slot ID");
        }
        self.slot_states[slot_id as usize] = SlotState::default();
        self.free_slots.push(slot_id);
    }

    fn get_slot_state(&self, slot_id: SlotStateId) -> Option<&SlotState> {
        self.slot_states.get(slot_id as usize)
    }

    fn set_slot_state(&mut self, slot_id: SlotStateId, value: Value) {
        if slot_id as usize >= self.slot_states.len() {
            panic!("Invalid slot ID");
        }
        let state = &mut self.slot_states[slot_id as usize];
        state.storage = value;
    }

    fn has_azimuth(&self, azimuth: AzimuthId) -> bool {
        for (azimuth_state, _) in self.slot_mapping.clone() {
            if azimuth_state.azimuth == azimuth { return true }
        }
        false
    }

    fn remove_azimuth(&mut self, azimuth: AzimuthId)  -> Option<MappingTo> {
        for i in (0..self.slot_mapping.len()).rev() {
            if self.slot_mapping[i].0.azimuth == azimuth {
                let id = self.slot_mapping.remove(i).1;
                return Some(id);
            }
        }
        None
    }

    fn get_slot_state_id(&self, azimuth: AzimuthId) -> Option<MappingTo> {
        for (azimuth_state, slot_state_id) in self.slot_mapping.clone() {
            if azimuth_state.azimuth == azimuth { return Some(slot_state_id) }
        }
        None
    }

    fn get_slot_state_id_mut(&mut self, azimuth: AzimuthId) -> Option<&mut MappingTo> {
        for (azimuth_state, slot_state_id) in &mut self.slot_mapping {
            if azimuth_state.azimuth == azimuth { return Some(slot_state_id) }
        }
        None
    }

    fn set_mapping(&mut self, azimuth: AzimuthId, mapping: MappingTo) {
        for (azimuth_state, slot_state_id) in &mut self.slot_mapping {
            if azimuth_state.azimuth == azimuth {
                *slot_state_id = mapping;
                break
            }
        }
    }

    fn set_value(&mut self, span:Span, azimuth:AzimuthId, value:Value) -> Result<Value, RuntimeError> {
        for (azimuth_state, mapping_to) in self.slot_mapping.clone() {
            if azimuth_state.azimuth == azimuth {

                if !value.kind().is_assignable_from(azimuth_state.value_type.clone()) {
                    return Err(RuntimeError::Error{span, message: format!("AAA Type mismatch: {:?} not assignable from {:?}", value.kind(), azimuth_state.value_type)});
                }

                // Slot found
                let slot_state_id = match mapping_to {
                    MappingTo::Slot(id) => id,
                    _ => panic!("AAAAA"),
                };
                self.set_slot_state(slot_state_id, value.clone());
                return Ok(value);
            }
        }
        return Err(RuntimeError::Error{span, message: format!("No such azimuth {:?} to assign to {:?}", azimuth, value)});
    }
}
     
#[derive(Debug, Clone)]
pub struct CallStackFunction {
    pub id: AzimuthId,
    pub span: Span,
    pub arguments: Vec<Value>,
}

// Runtime
pub struct Runtime {
    shapes: HashMap<u32, ShapeInfo>,    
    azimuths: HashMap<u32, AzimuthInfo>,
    objects: HashMap<ObjectId, Object>,
    locals: HashMap<LocalId, Value>,
    obj_ref_count: HashMap<ObjectId, u32>,
    local_ref_count: HashMap<LocalId, u32>,
    call_stack: Vec<CallStackFunction>,
}

impl Runtime {
    fn new(shapes: HashMap<u32, ShapeInfo>, azimuths: HashMap<u32, AzimuthInfo>) -> Self {
        let global = Object {
                id: 0,
                name: format!("global"),
                flags: ObjectFlags {
                    sealed: true,
                },
                slot_mapping: Vec::new(),
                slot_states: Vec::new(),
                free_slots: Vec::new(),
                next_slot_state_id: 1,
            };
        let mut runtime = Runtime {
            shapes,
            azimuths,
            objects: HashMap::new(),
            locals: HashMap::new(),
            obj_ref_count: HashMap::new(),
            local_ref_count: HashMap::new(),
            call_stack: Vec::new(),
        };
        runtime.objects.insert(0, global);
        runtime
    }

    fn init_static_instances(&mut self, static_info: HashMap<u32, (ObjectInfo, Vec<AzimuthId>)>) -> Result<(), RuntimeError> {
        let shapes = {
            let mut namespaceinfo = Vec::new();

            let mut namespaces : Vec<(&u32, &(ObjectInfo, Vec<AzimuthId>))> = static_info.iter().collect();
            namespaces.sort_by(|(a,_), (b,_)| (a).cmp(b));

            for (_, (info, azimuths)) in namespaces {
                let mut raw_azimuths = Vec::new();

                for az_id in azimuths {
                    let azimuth = self.get_azimuth(*az_id);
                    
                    //println!("Found azimuth id {}: {:?}\n", az_id, azimuth);

                    if !azimuth.flags.is_static { continue }

                    raw_azimuths.push((*az_id, azimuth.value_type.clone(), azimuth.default_value.clone()));
                }
                
                namespaceinfo.push((info.clone(), raw_azimuths));
            }
            namespaceinfo
        };

        let evaluated_shapes = {
            let mut evaluated_shapes = Vec::new();
            for (static_info, raw_azimuths) in shapes {
                let mut azimuths = Vec::new();

                for (id, value_type, value) in raw_azimuths {
                    let mut span = Span::default();

                    //println!("Evaluating {}: {:?}\n", id, value);

                    let evaluated = match value {
                        None => None,
                        Some(expr) => {
                            span = (*expr).span().clone();
                            Some(executor::evaluate(self, *expr)?)
                        }
                    };
                    azimuths.push((id, value_type, evaluated, span));
                }

                evaluated_shapes.push((static_info, azimuths));
            }
            evaluated_shapes
        };
        
        for (static_info, azimuths) in evaluated_shapes {
            let id = static_info.id;
            self.create_object(static_info);
            let object = self.get_object_mut(id);

            for (az_id, value_type, default_value, span) in azimuths {
                let az_state = AzimuthState { azimuth: az_id, value_type: value_type };
                let state_id = object.allocate_slot();
                object.slot_mapping.push((az_state, MappingTo::Slot(state_id)));
                match default_value {
                    Some(value) => {
                        object.set_value(span, az_id, value)?;
                    }
                    None => {}
                }
            }
        }

        Ok(())
    }

    fn create_object(&mut self, info: ObjectInfo) {
        let id = info.id;
        let object = Object {
            id: id,
            name: info.name,
            flags: ObjectFlags {
                sealed: false,
            },
            slot_mapping: Vec::new(),
            slot_states: Vec::new(),
            free_slots: Vec::new(),
            next_slot_state_id: 0,
        };
                
        println!("Created object with ID {}, '{}'", object.id, object.name);
        self.objects.insert(id, object);
        self.obj_ref_count.insert(id, 0);
    }

    fn attach_slot(&mut self, span:Span, object_id: ObjectId, azimuth_id: AzimuthId, remap: Option<AzimuthId>, generic: Option<ValueKind>, mapping_kind: Option<MappingKind>, default: Option<Value>) -> Result<bool, RuntimeError> {
        let azimuth = self.get_azimuth(azimuth_id).clone();
        let shape = match self.get_shape(azimuth.shape_id) {
            Some(info) => info,
            None => return Err(RuntimeError::Error{span, message: format!("Shape not found for azimuth: {:?}", azimuth)}),
        };
        let target_az_id;
        
        if let Some(remap_id) = remap {
            let remap_slot = self.get_azimuth(remap_id);
            //if !generic.clone().unwrap_or(azimuth.value_type.clone()).is_assignable_from_runtime(remap_slot.value_type.clone(), self) { 
            //    return Err(RuntimeError::Error{span, message: format!("Code 14: Type mismatch: {:?} with kind {:?} not assignable from {:?}", azimuth.name, azimuth.value_type, remap_slot.value_type)}); 
            //}
            
            target_az_id = remap_id;
            println!("- Will remap {} -> {} (explicit)", azimuth.name, remap_slot.name);
        } else {

            // New slot
            target_az_id = azimuth_id;
        }
        let target_az = self.get_azimuth(target_az_id).clone();

        //println!("{:?}", self.objects);
        let object = self.get_object(object_id);

        if object.has_azimuth(azimuth_id) {
            //return Ok(false)
            panic!("Slot already attached to object: {} -> {:?}", azimuth_id, object.slot_mapping.iter().find(|(st, map)| st.azimuth == azimuth_id))
        }

        let mut set_default = false;

        // Check for present slot mappings
        let state_mapping = if target_az_id != azimuth_id {

            // Target exists but not present; attach
            if !object.has_azimuth(target_az_id) {
                println!("- First attaching target slot {}", target_az.name);
                self.attach_slot(span.clone(), object_id, target_az_id, None, generic.clone(), Some(MappingKind::Strict), None)?;
            }

            let object = self.get_object_mut(object_id); // Reference again
            let mapping_to = object.get_slot_state_id(target_az_id).unwrap();
            
            println!("- Mapping {} to existing local slot {:?} (shared with {})", azimuth.name, mapping_to, target_az.name);
            
            match mapping_kind {
                None | Some(MappingKind::Strict) => MappingTo::Map(target_az_id),
                _ => {
                    // Create chain in target
                    let after = matches!(mapping_kind, Some(MappingKind::After));
                    
                    let old_target = object.get_slot_state_id(target_az_id);
                    let (mut chain, base) = match old_target {

                        // Add to existing chain
                        Some(MappingTo::Chain(chain, base)) => (chain, base),

                        // Create new chain
                        Some(mapping) => {
                            let old = mapping.clone();
                            ([old].to_vec(), 0)
                        }

                        None => panic!("Old target {} somehow had no mapping: {:?}", target_az_id, object)
                    };

                    let index = if after { base + 1 } else { base };
                    chain.insert(index, MappingTo::Map(azimuth_id));

                    let new_base: usize = if after { base } else { base + 1 };
                    let new_target = MappingTo::Chain(chain, new_base);
                    object.set_mapping(target_az_id, new_target);

                    // If static, make link
                    if azimuth.flags.is_static {
                        let shape = self.get_shape(azimuth.shape_id).unwrap();
                        MappingTo::Link(shape.static_id.clone().unwrap().id, azimuth.id)
                        
                    // Create new 
                    } else {
                        let object = self.get_object_mut(object_id);
                        let new_slot = object.allocate_slot();
                        println!("- Allocated local slot {} for {}", new_slot, azimuth.name);
                    
                        set_default = true;
                        MappingTo::Slot(new_slot)
                    }   
                }
            }
            
        } else {
            // If static, make link
            if azimuth.flags.is_static {
                let shape = self.get_shape(azimuth.shape_id).unwrap();
                MappingTo::Link(shape.static_id.clone().unwrap().id, azimuth.id)
                
            // Create new 
            } else {
                let object = self.get_object_mut(object_id);
                let new_slot = object.allocate_slot();
                println!("- Allocated local slot {} for {}", new_slot, azimuth.name);
            
                set_default = true;
                MappingTo::Slot(new_slot)
            }
        };

        let object = self.get_object_mut(object_id);
        let azimuth_state = AzimuthState{azimuth: azimuth_id, value_type: generic.unwrap_or(azimuth.value_type.clone())};
        object.slot_mapping.push((azimuth_state, state_mapping));

        // Set default
        match default {
            Some(default) if set_default => {
                self.set_slot_value(span.clone(), object_id, azimuth_id, default)?;
            }
            _ => {}
        }

        Ok(true)
    }

    fn detach_slot(&mut self, object_id: ObjectId, slot: AzimuthId) {
        // Do not detach locked slot
        if self.get_azimuth(slot).flags.is_locked { return }

        let object = self.get_object_mut(object_id);

        match object.remove_azimuth(slot) {
            Some(MappingTo::Slot(state_id)) => {
                // Check if any other slot maps to the same state
                let mut still_mapped = false;
                for (_, mapping) in &object.slot_mapping {
                    match mapping {
                        MappingTo::Slot(id) => {
                            if *id == state_id {
                                still_mapped = true;
                                break
                            }
                        }
                        _ => continue
                    }
                }

                if !still_mapped {
                    object.free_slot(state_id);
                }
                
                let azimuth = self.get_azimuth(slot);
                println!("Azimuth removed: {:?}", azimuth.name);
            }
            Some(_) => {
                let azimuth = self.get_azimuth(slot);
                println!("Azimuth mapping removed: {:?}", azimuth.name);
            }
            None => {
                let azimuth = self.get_azimuth(slot);
                println!("Azimuth not attached to object: {:?}", azimuth.name);
            }
        }
    }

    fn remap_slot(&mut self, span:Span, object_id: ObjectId, to: AzimuthId, from: AzimuthId) -> Result<(), RuntimeError> {
        let to_azimuth = self.get_azimuth(to).clone();
        let object = self.get_object_mut(object_id);

        if let Some(to_id) = object.get_slot_state_id(to) {
            let azimuth_state = AzimuthState{azimuth: from.clone(), value_type: to_azimuth.value_type.clone()};
            object.slot_mapping.push((azimuth_state, to_id));
            Ok(())
        } else {
            Err(RuntimeError::Error{span, message: format!("Target slot {} not attached to object", to_azimuth.name)})
        }
    }

    fn attach_shape(&mut self, span:Span, object_id: ObjectId, shape_id: ShapeInstance) -> Result<(), RuntimeError> {
        self.attach_shape_with_remap(span, object_id, shape_id, Vec::new())
    }

    fn detach_shape(&mut self, span:Span, object_id: ObjectId, shape_id: ShapeInstance) -> Result<(), RuntimeError> {
        let (shape_name, azimuths) = {
            let shape = match self.get_shape(shape_id.id) {
                Some(info) => info,
                None => return Err(RuntimeError::Error{span, message: format!("Shape not found: {:?}", shape_id)}),
            };
            (shape.name.clone(), shape.azimuths.clone())
        };

        println!("Detaching shape {} from object {}", shape_name, object_id);

        for azimuth_id in azimuths {
            let az = self.get_azimuth(azimuth_id);

            self.detach_slot(object_id, azimuth_id);
        }
        Ok(())
    }

    fn attach_shape_with_remap(&mut self, span:Span, object_id: ObjectId, shape_id: ShapeInstance, remap: Vec<Mapping>,) -> Result<(), RuntimeError> {
        let (shape_name, azimuths, parents, num_generics) = {
            let shape = match self.get_shape(shape_id.id) {
                Some(info) => info,
                None => return Err(RuntimeError::Error{span, message: format!("Shape not found: {:?}", shape_id)}),
            };

            if shape.generics.len() != shape_id.generics.len() {
                return Err(RuntimeError::Error{span, message: format!("Invalid number of generic arguments: {:?}", shape_id.generics)});
            }

            (shape.name.clone(), shape.azimuths.clone(), shape.parent_ids.clone(), shape.generics.len())
        };

        println!("Remap: {:?}", remap);

        println!(
            "Attaching shape {} to object {}",
            shape_name, object_id
        );

        for az_id in azimuths {
            let azimuth = self.get_azimuth(az_id).clone();

            let generic_type = match azimuth.value_type {
                ValueKind::Generic(generic,_) => {
                    shape_id.generics.get(generic as usize).cloned()
                }
                _ => None,
            };

            let default_value = match azimuth.default_value {
                None => None,
                Some(expr) => Some(executor::evaluate(self, *expr)?),
            };

            // Mapping
            if let Some(mapping) = remap.iter().find(|m| m.from == az_id) {
                self.attach_slot(span.clone(), object_id, az_id, Some(mapping.to), generic_type, Some(mapping.kind.clone()), default_value)?;

            } else {
                if generic_type.is_none() && azimuth.flags.is_static { continue; }
                self.attach_slot(span.clone(), object_id, az_id,None, generic_type, None, default_value)?;
            }
        }
        
        for parent in parents{
            println!("Attaching inheritance: {:?}", parent);
            self.attach_shape_with_remap(span.clone(), object_id, ShapeInstance{ id:parent.id, generics:shape_id.generics.clone() }, remap.clone())?;
        }

        Ok(())
    }

    fn get_object(&self, id: ObjectId) -> &Object {
        self.objects.get(&id).expect(format!("Object {} not found", id).as_str())
    }

    fn get_object_mut(&mut self, id: ObjectId) -> &mut Object {
        self.objects.get_mut(&id).expect(format!("Object {} not found", id).as_str())
    }

    fn get_azimuth(&self, id: AzimuthId) -> &AzimuthInfo {
        self.azimuths.get(&id).expect(format!("Azimuth {} not found", id).as_str())
    }

    fn get_azimuth_mut(&mut self, id: AzimuthId) -> &AzimuthInfo {
        self.azimuths.get_mut(&id).expect(format!("Azimuth {} not found", id).as_str())
    }

    fn get_shape(&self, id: ShapeId) -> Option<&ShapeInfo> {
        self.shapes.get(&id)
    }

    fn get_shape_mut(&mut self, id: ShapeId) -> Option<&mut ShapeInfo> {
        self.shapes.get_mut(&id)
    }

    fn get_local(&self, id: LocalId) -> Option<&Value> {
        //println!("searching for local {}...", id);
        self.locals.get(&id)
    }

    fn reserve_local(&mut self, id: LocalId, value:Value) {
        //println!("reserving local {} to {:?}...", id, value);
        match value {
            Value::Object(id, _) => { 
                self.obj_ref_count.insert(id, self.obj_ref_count.get(&id).unwrap_or(&0) + 1); 
            },
            _ => {}
        }
        self.locals.insert(id, value);
        self.local_ref_count.insert(id, 1);
    }

    fn clear_local(&mut self, id: LocalId) {
        //println!("clearing local {}...", id);
        match self.locals.remove(&id) {
            Some(Value::Object(id, _)) => {
                let ref_count = self.obj_ref_count.get(&id).unwrap_or(&0);
                if *ref_count <= 1 {
                    self.clear_object(id);
                } else {
                    self.obj_ref_count.insert(id, ref_count - 1);
                }
            },
            _ => {}
        }
    }

    fn ref_local(&mut self, id: LocalId, errorcode: i32) {
        match self.local_ref_count.get(&id) {
            None => panic!("{}: The ref thing happened: {}", errorcode, id),
            Some(count) => {
                self.local_ref_count.insert(id, count + 1);
            }
        }
    }

    fn deref_local(&mut self, id: LocalId, errorcode: i32) {
        match self.local_ref_count.get(&id) {
            None => panic!("{}: The deref thing happened: {}", errorcode, id),
            Some(count) => {
                if *count == 1 {
                    self.clear_local(id);
                } else {
                    self.local_ref_count.insert(id, count - 1);
                }
            }
        }
    }

    fn clear_object(&mut self, id: ObjectId) {
        self.obj_ref_count.remove(&id);
        self.objects.remove(&id);
    }

    fn clear_locals(&mut self, ids: Vec<LocalId>) {
        for id in ids {
            self.clear_local(id);
        }
    }

    fn deref_locals(&mut self, ids: Vec<LocalId>, errorcode: i32) {
        for id in ids {
            self.deref_local(id, errorcode);
        }
    }

    fn is_shape(&self, object_id: ObjectId, shape_id: ShapeId) -> bool {
        let object = self.get_object(object_id);
        let shape = self.get_shape(shape_id).unwrap();

        shape.azimuths.iter().map(|id| self.get_azimuth(*id)).all(|slot| {
            if slot.flags.is_static {
                //let static_object = self.get_object(shape.static_id);
                object.has_azimuth(slot.id) //|| static_object.has_azimuth(slot.id)
            } else {
                object.has_azimuth(slot.id)
            }
        })
    }

    fn get_slot_value(&self, object_id: ObjectId, slot: AzimuthId) -> Option<Value> {
        let object = self.get_object(object_id);
        if let Some(mapping) = object.get_slot_state_id(slot) {
            match mapping {
                MappingTo::Slot(id) => {
                    if let Some(state) = object.get_slot_state(id) {
                        return Some(state.storage.clone());
                    }
                }
                MappingTo::Link(other_object_id, other_azimuth) => {
                    return self.get_slot_value(other_object_id, other_azimuth);
                }
                MappingTo::Map(other_azimuth) => {
                    return self.get_slot_value(object_id, other_azimuth);
                }
                MappingTo::Chain(azimuths, _) => {
                    let azimuth = self.get_azimuth(slot);
                    return Some(Value::FunctionChain(azimuths.clone(), azimuth.value_type.clone()))
                }
                MappingTo::Expression(expr) => {
                    //let result = executor::evaluate_place(self, expr);
                    //return Some(&result.unwrap())
                    todo!()
                }
            }
        }

        // Static
        let azimuth = self.get_azimuth(slot);
        if azimuth.flags.is_static {
            //println!("Static azimuth found. Getting static Id");
            let static_id = match &self.get_shape(azimuth.shape_id).unwrap().static_id {
                Some(info) => {
                    //println!("Id: {}", info.id);
                    info.id
                }
                None => return None,
            };
            if static_id == object_id { return None; }
            
            return self.get_slot_value(static_id, slot);
        }

        None
    }

    fn get_slot_value_mut(&mut self, object_id: ObjectId, slot: AzimuthId) -> Option<&mut Value> {
        let object_info = self.get_object(object_id);
        if let Some(mapping) = object_info.get_slot_state_id(slot.clone()) {
            match mapping {
                MappingTo::Slot(state_id) => {
                    let object = self.get_object_mut(object_id);
                    if (state_id as usize) < object.slot_states.len() {
                        return Some(&mut object.slot_states[state_id as usize].storage);
                    }
                }
                MappingTo::Link(other_object_id, other_azimuth) => {
                    return self.get_slot_value_mut(other_object_id, other_azimuth);
                }
                MappingTo::Map(other_azimuth) => {
                    return self.get_slot_value_mut(object_id, other_azimuth);
                }
                MappingTo::Chain(azimuths, _) => {
                    todo!()
                }
                MappingTo::Expression(expr) => {
                    todo!()
                }
            }
        }
        None
    }

    fn get_slot_value_array_element(&self, object_id: ObjectId, slot: AzimuthId, index: usize) -> Option<Value> {
        if let Some(Value::Array(arr, _)) = self.get_slot_value(object_id, slot) {
            return arr.get(index).cloned();
        }
        None
    }

    fn set_slot_value(&mut self, span:Span, object_id: ObjectId, slot: AzimuthId, value: Value) -> Result<(), RuntimeError> {
        match value {
            Value::Object(id, _) => { 
                self.obj_ref_count.insert(id, self.obj_ref_count.get(&id).unwrap_or(&0) + 1); 
            },
            _ => {}
        }
        
        let old_value = self.get_slot_value(object_id, slot);
        match old_value {
            Some(Value::Object(id, _)) => {
                let ref_count = self.obj_ref_count.get(&id).unwrap_or(&0);
                if *ref_count <= 1 {
                    self.clear_object(id);
                } else {
                    self.obj_ref_count.insert(id, ref_count - 1);
                }
            },
            _ => {}
        }
        
        let azimuth_name = self.get_azimuth(slot).name.clone();
        
        let object = self.get_object_mut(object_id);
        object.set_value(span, slot, value.clone())?;

        println!("Set obj {}.{} to {}", object.name, azimuth_name, value);
        Ok(())
    }

    fn set_slot_value_array(&mut self, span:Span, object_id: ObjectId, slot: AzimuthId, values: Vec<Value>, kind: ValueKind) -> Result<(), RuntimeError> {
        self.set_slot_value(span, object_id, slot, Value::Array(values.into_iter().collect(), kind))
    }
    fn set_slot_value_array_element(&mut self, object_id: ObjectId, slot: AzimuthId, index: usize, value: Value) {
        if let Some(Value::Array(arr, _)) = self.get_slot_value_mut(object_id, slot) {
            if index < arr.len() {
                arr[index] = value;
            } else {
                panic!("Array index out of bounds");
            }
        } else {
            panic!("Slot does not contain an array");
        }
    }

    fn push_array_element(&mut self, object_id: ObjectId, slot: AzimuthId, value: Value) {
        if let Some(Value::Array(arr, _)) = self.get_slot_value_mut(object_id, slot) {
            arr.push(value);
        } else {
            panic!("Slot does not contain an array")
        }
    }
    fn insert_array_element(&mut self, object_id: ObjectId, slot: AzimuthId, index: usize, value: Value) {
        if let Some(Value::Array(arr, _)) = self.get_slot_value_mut(object_id, slot) {
            if index < arr.len() {
                arr.insert(index, value);
            } else {
                panic!("Array index out of bounds");
            }
        } else {
            panic!("Slot does not contain an array");
        }
    }
    fn remove_array_element(&mut self, object_id: ObjectId, slot: AzimuthId, index: usize) {
        if let Some(Value::Array(arr, _)) = self.get_slot_value_mut(object_id, slot) {
            if index < arr.len() {
                arr.remove(index);
            } else {
                panic!("Array index out of bounds");
            }
        } else {
            panic!("Slot does not contain an array");
        }
    }

    fn seal(&mut self, object_id: ObjectId) {
        let object = self.get_object_mut(object_id);
        object.flags.sealed = true;
    }

    fn unseal(&mut self, object_id: ObjectId) {
        let object = self.get_object_mut(object_id);
        object.flags.sealed = false;
    }

    fn get_intrinsic_static_id(&self, kind: ValueKind) -> Option<ObjectId> {
        for shape in self.shapes.values() {
            if shape.name.contains(match kind {
                ValueKind::Number(NumKind::Int32) => "Int32",
                ValueKind::Bool => "Bool",
                ValueKind::String => "String",
                ValueKind::Array(_) => "Array",
                ValueKind::Set(_) => "Set",
                ValueKind::Dict(_,_) => "Dict",
                ValueKind::Range(_) => "Range",
                _ => todo!()
            }) {
                return Some(shape.id)
            }
        }
        None
    }

    fn print_object(&self, object_id: ObjectId) {
        let object = self.get_object(object_id);
        println!("\nObject {}: '{}'", object.id, object.name);

        for state_id in 0..=object.slot_states.len(){
            if let Some(state) = object.get_slot_state(state_id as SlotStateId) {
                
                print!("- Local slot {}: ", state_id);
                println!("{}", &state.storage);

            }   
        }
        for (azimuth, mapping) in &object.slot_mapping{
            let name = self.get_azimuth(azimuth.azimuth).name.clone();
            println!("- Mapping: {} -> {:?}", name, mapping);
        }
        println!();
    }

    fn get_stack_trace(&self) -> String {
        let mut output = String::new();
        for func in &self.call_stack {

            let mut args = String::new();
            for arg in &func.arguments {
                args += &arg.to_string();
            }

            output += format!("\nat [{},{}] \"{}\", with arguments: [{}],",
                func.span.line, func.span.column,
                self.get_azimuth(func.id).name,
                args
            ).as_str();
        }
        output
    }

    fn panic_stack_trace(&self, error: RuntimeError) {
        panic!("Program failed: {} {}", error, self.get_stack_trace())
    }

}

//fn print_hello(params: FunctionParams) -> Value {
//    println!("Hello from native function! Caller: {}", params.caller);
//    Value::Single(PrimitiveValue::None)
//}

fn main() {
    let loader = match loader::load("/workspaces/SlotBox/src/lang", "atlas.atls") {
        Err(error) => panic!("Could not load atlas.atls: {}",error),
        Ok(loader) => loader,
    };

    let (resolved_ast, analyzer) = match analyzer::analyze(loader){
        Err(error) => panic!("Could not compile:\n{}", error),
        Ok(resolved_ast) => resolved_ast,
    };

    let mut runtime = Runtime::new(analyzer.shapes, analyzer.azimuths);
    match executor::execute(&mut runtime, resolved_ast, analyzer.namespace_static_info){
        Err(error) => runtime.panic_stack_trace(error),
        _ => println!("Program executed successfully."),
    };
}