use std::{env, io, string, time::{SystemTime, UNIX_EPOCH}};
use rand::Rng;

use crate::{NumKind, Number, Runtime, Value, ValueKind, analyzer::AzimuthInfo, executor::RuntimeError, lexer::Span, parser::{Expression, ParseError, ShapeExpression, Statement}};

pub struct IntrinsicParameters<'a> {
    pub span: Span,
    pub args: Vec<Value>,
    pub runtime: &'a mut Runtime,
    pub azimuth: Option<AzimuthInfo>,
}

pub type IntrinsicOp = fn(IntrinsicParameters) -> Result<Value, RuntimeError>;

fn array_append(input: IntrinsicParameters) -> Result<Value, RuntimeError> {
    let array_member = &input.args[0];
    let add = &input.args[1];

    match array_member {
        Value::Pointer(obj, az, _) => {
            input.runtime.push_array_element(*obj, *az, add.clone());
            Ok(true.into())
        }
        Value::Local(id, kind) if kind.is_assignable_from(ValueKind::Array(Box::new(ValueKind::Dyn))) => {
            let local = match input.runtime.locals.get_mut(id).unwrap() {
                Value::Array(array, _) => array,
                _ => unreachable!()
            };
            local.push(add.clone());
            Ok(true.into())
        }
        other => Err(RuntimeError::TypeMismatch{span:input.span, found: other.clone(), expected: ValueKind::Array(Box::new(ValueKind::None)) }),
    }
}

fn array_insert(input: IntrinsicParameters) -> Result<Value, RuntimeError> {
    let array_member = &input.args[0];
    let add = &input.args[1];
    let index = &input.args[2];

    match (array_member, index) {
        (Value::Pointer(obj, az, _), Value::Number(num)) => {
            input.runtime.insert_array_element(*obj, *az, num.to_i32().unwrap() as usize, add.clone());
            Ok(true.into())
        }
        (other, index) => Err(RuntimeError::TypeMismatch{span:input.span, found: other.clone(), expected: ValueKind::Array(Box::new(ValueKind::None)) }),
    }
}

fn array_remove(input: IntrinsicParameters) -> Result<Value, RuntimeError> {
    let array_member = &input.args[0];
    let index = &input.args[1];

    match (array_member, index) {
        (Value::Pointer(obj, az, _), Value::Number(num)) => {
            input.runtime.remove_array_element(*obj, *az, num.to_i32().unwrap() as usize);
            Ok(true.into())
        }
        (other, index) => Err(RuntimeError::TypeMismatch{span:input.span, found: other.clone(), expected: ValueKind::Array(Box::new(ValueKind::None)) }),
    }
}

fn math_sqrt(input: IntrinsicParameters) -> Result<Value, RuntimeError> {
    let operand = &input.args[0];

    match operand {
        //Value::Number(Number::Int32(val)) => sqrt()

        other => Err(RuntimeError::TypeMismatch{span:input.span, found: other.clone(), expected: ValueKind::Number(NumKind::Any) }),
    }
}

fn io_readline(input: IntrinsicParameters) -> Result<Value, RuntimeError> {
    let mut result = String::new();
    match io::stdin().read_line(&mut result) {
        Err(_) => Ok(Value::None),
        _ => Ok(result.trim().to_string().into()),
    }
}

fn io_args(input: IntrinsicParameters) -> Result<Value, RuntimeError> {
    let cmd_args: Vec<String> = env::args().collect();
    let mut result = Vec::new();
    for arg in cmd_args {
        result.push(arg.into())
    }
    Ok(Value::Array(result, ValueKind::Array(Box::new(ValueKind::String))))
}

fn random_int(input: IntrinsicParameters) -> Result<Value, RuntimeError> {
    let mut rng = rand::thread_rng();
    let random: i32 = rng.r#gen();
    Ok(random.into())
}

fn random_range(input: IntrinsicParameters) -> Result<Value, RuntimeError> {
    let mut rng = rand::thread_rng();

    let from = &input.args[0];
    let to = &input.args[1];

    match (to, from) {
        (Value::Number(to), Value::Number(from)) => {
            let random: i32 = rng.gen_range(from.to_i32().unwrap()..=to.to_i32().unwrap());
            Ok(random.into())
        }
        _ => Err(RuntimeError::TypeMismatch{span:input.span, found: to.clone(), expected: ValueKind::Number(NumKind::Any) })
    }
}

fn string_upper(input: IntrinsicParameters) -> Result<Value, RuntimeError> {
    let val = &input.args[0];
    match val {
        Value::String(string) => Ok(Value::String(string.to_uppercase())),
        other => Err(RuntimeError::TypeMismatch{span:input.span, found:other.clone(), expected: ValueKind::String })
    }
}

fn string_lower(input: IntrinsicParameters) -> Result<Value, RuntimeError> {
    let val = &input.args[0];
    match val {
        Value::String(string) => Ok(Value::String(string.to_lowercase())),
        other => Err(RuntimeError::TypeMismatch{span:input.span, found:other.clone(), expected: ValueKind::String })
    }
}

fn string_trim(input: IntrinsicParameters) -> Result<Value, RuntimeError> {
    let val = &input.args[0];
    match val {
        Value::String(string) => Ok(Value::String(string.trim().to_string())),
        other => Err(RuntimeError::TypeMismatch{span:input.span, found:other.clone(), expected: ValueKind::String })
    }
}

fn benchmarking_get_time(input: IntrinsicParameters) -> Result<Value, RuntimeError> {
    let now = SystemTime::now();
    let duration_since_epoch = now
        .duration_since(UNIX_EPOCH)
        .expect("SystemTime went backwards");
    let seconds: u64 = duration_since_epoch.as_millis().try_into().expect(format!("Millis to big: {:?}", duration_since_epoch).as_str());
    Ok(Value::Number(Number::UInt64(seconds)))
}

fn range_to_array(input: IntrinsicParameters) -> Result<Value, RuntimeError> {
    let val = &input.args[0];
    let (from, to, by, inclusive) = match val {
        Value::Range(from, to, by, inclusive, _) => (from.to_i32().unwrap(), to.to_i32().unwrap(), by.to_i32().unwrap(), *inclusive),
        other => return Err(RuntimeError::TypeMismatch{span:input.span, found:other.clone(), expected: ValueKind::Range(NumKind::Any) })
    };

    let values: Vec<Value> = if from <= to {
        if inclusive {
            (from..=to).step_by(by as usize).map(|i| i.into()).collect()
        } else {
            (from..to).step_by(by as usize).map(|i| i.into()).collect()
        }
    } else {
        if inclusive {
            (to..=from).rev().step_by(-by as usize).map(|i| i.into()).collect()
        } else {
            (to..from).rev().step_by(-by as usize).map(|i| i.into()).collect()
        }
    };

    Ok(Value::Array(values, ValueKind::Number(NumKind::Int32)))
}

fn range_create_inclusive(input: IntrinsicParameters) -> Result<Value, RuntimeError> {
    let start = &input.args[0];
    let end = &input.args[1];
    let by = &input.args[2];
    match (start, end, by) {
        (Value::Number(start), Value::Number(end), Value::Number(by)) => 
            Ok(Value::Range(start.clone(), end.clone(), by.clone(), true, NumKind::Int32)),
        (other, _,_) => Err(RuntimeError::TypeMismatch{span:input.span, found:other.clone(), expected: ValueKind::Number(NumKind::Any) })
    }
}

fn range_create_exclusive(input: IntrinsicParameters) -> Result<Value, RuntimeError> {
    let start = &input.args[0];
    let end = &input.args[1];
    let by = &input.args[2];
    match (start, end, by) {
        (Value::Number(start), Value::Number(end), Value::Number(by)) => 
            Ok(Value::Range(start.clone(), end.clone(), by.clone(), false, NumKind::Int32)),
        (other, _,_) => Err(RuntimeError::TypeMismatch{span:input.span, found:other.clone(), expected: ValueKind::Number(NumKind::Any) })
    }
}

fn set_to_array(input: IntrinsicParameters) -> Result<Value, RuntimeError> {
    let set = &input.args[0];
    match set {
        Value::Set(vec, kind) => 
            Ok(Value::Array(vec.clone(), kind.clone())),
        other => Err(RuntimeError::TypeMismatch{span:input.span, found:other.clone(), expected: ValueKind::Set(Box::new(ValueKind::None)) })
    }
}

fn set_contains(input: IntrinsicParameters) -> Result<Value, RuntimeError> {
    let set = &input.args[0];
    let value = &input.args[1];
    match set {
        Value::Set(vec, _) => 
            Ok(Value::Bool(vec.contains(value))),
        other => Err(RuntimeError::TypeMismatch{span:input.span, found:other.clone(), expected: ValueKind::Set(Box::new(ValueKind::None)) })
    }
}

fn dict_get(input: IntrinsicParameters) -> Result<Value, RuntimeError> {
    let dict = &input.args[0];
    let key = &input.args[1];
    match dict {
        Value::Dict(vec, _, _) => {
            for (k, val) in vec {
                if k == key { return Ok(val.clone()) }
            }
            Ok(Value::None)
        }
        other => Err(RuntimeError::TypeMismatch{span:input.span, found:other.clone(), expected: ValueKind::Set(Box::new(ValueKind::None)) })
    }
}

fn dict_values(input: IntrinsicParameters) -> Result<Value, RuntimeError> {
    let dict = &input.args[0];
    match dict {
        Value::Dict(vec, _, val_kind) => {
            Ok(Value::Array(vec.iter().map(|(_,v)|v.clone()).collect(), val_kind.clone()))
        }
        other => Err(RuntimeError::TypeMismatch{span:input.span, found:other.clone(), expected: ValueKind::Set(Box::new(ValueKind::None)) })
    }
}

fn dict_keys(input: IntrinsicParameters) -> Result<Value, RuntimeError> {
    let dict = &input.args[0];
    match dict {
        Value::Dict(vec, key_kind, _) => {
            Ok(Value::Set(vec.iter().map(|(k,_)|k.clone()).collect(), key_kind.clone()))
        }
        other => Err(RuntimeError::TypeMismatch{span:input.span, found:other.clone(), expected: ValueKind::Set(Box::new(ValueKind::None)) })
    }
}

fn runtime_print_locals(input: IntrinsicParameters) -> Result<Value, RuntimeError> {
    println!("{:?}", input.runtime.locals);
    Ok(Value::None)
}
fn runtime_print_objects(input: IntrinsicParameters) -> Result<Value, RuntimeError> {
    println!("{:?}", input.runtime.objects);
    Ok(Value::None)
}
fn runtime_print_stack(input: IntrinsicParameters) -> Result<Value, RuntimeError> {
    println!("{}",input.runtime.get_stack_trace());
    Ok(Value::None)
}

pub fn lookup(span: Span, name: String) -> Result<IntrinsicOp, ParseError> {
    match name.as_str() {
        "Array::Append" => Ok(array_append),
        "Array::Add" => Ok(array_insert),
        "Array::Remove" => Ok(array_remove),
        "Array::Get" => Ok(todo),
        "Set::ToArray" => Ok(set_to_array),
        "Set::Add" => Ok(todo),
        "Set::Remove" => Ok(todo),
        "Set::Contains" => Ok(set_contains),
        "Dict::Keys" => Ok(dict_keys),
        "Dict::Values" => Ok(dict_values),
        "Dict::Set" => Ok(todo),
        "Dict::Remove" => Ok(todo),
        "Dict::Get" => Ok(dict_get),
        "Sqrt" => Ok(math_sqrt),
        "Args" => Ok(io_args),
        "ReadLine" => Ok(io_readline),
        "Int" => Ok(random_int),
        "Range" => Ok(random_range),
        "String::Upper" => Ok(string_upper),
        "String::Lower" => Ok(string_lower),
        "String::Trim" => Ok(string_trim),
        "Benchmarking::GetTime" => Ok(benchmarking_get_time),
        "Range::ToArray" => Ok(range_to_array),
        "Range::Create" => Ok(range_create_inclusive),
        "Range::CreateExclusive" => Ok(range_create_exclusive),
        "PrintObjects" => Ok(runtime_print_objects),
        "PrintStack" => Ok(runtime_print_stack),
        "PrintLocals" => Ok(runtime_print_locals),

        other => Err(ParseError::Error{span, message:format!("No intrinsic operation defined for {}", other)})
    }
}

fn todo(input: IntrinsicParameters) -> Result<Value, RuntimeError> {
    Err(RuntimeError::Error{span:input.span, message:format!("TODO")})
}