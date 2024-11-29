use crate::{jni_clear_ex, loader::jni_attach_vm, AutoLocal, JObjectAutoLocal};
use jni::{
    errors::Error,
    objects::{GlobalRef, JClass, JMethodID, JObject, JStaticMethodID, JValueOwned},
    signature::{Primitive, ReturnType},
    sys::{jboolean, jbyte, jchar, jdouble, jfloat, jint, jlong, jshort, jvalue},
    JNIEnv,
};
use std::sync::OnceLock;

/// Gets the value returned from the Java method; calls `jni_clear_ex()` for an error.
pub trait JValueGenGet<'a> {
    fn clear_ex(self) -> Result<(), Error>;
    fn get_object(self, env: &JNIEnv<'a>) -> Result<AutoLocal<'a>, Error>;
    fn get_boolean(self) -> Result<bool, Error>;
    fn get_byte(self) -> Result<jbyte, Error>;
    fn get_char(self) -> Result<jchar, Error>;
    fn get_short(self) -> Result<jshort, Error>;
    fn get_int(self) -> Result<jint, Error>;
    fn get_long(self) -> Result<jlong, Error>;
    fn get_float(self) -> Result<jfloat, Error>;
    fn get_double(self) -> Result<jdouble, Error>;
}

impl<'a> JValueGenGet<'a> for Result<JValueOwned<'a>, Error> {
    #[inline(always)]
    fn clear_ex(self) -> Result<(), Error> {
        self.map(|_| ()).map_err(jni_clear_ex)
    }

    #[inline(always)]
    fn get_object(self, env: &JNIEnv<'a>) -> Result<AutoLocal<'a>, Error> {
        self.and_then(|v| v.l())
            .map(|o| env.auto_local(o))
            .map_err(jni_clear_ex)
    }

    #[inline(always)]
    fn get_boolean(self) -> Result<bool, Error> {
        self.and_then(|v| v.z()).map_err(jni_clear_ex)
    }
    #[inline(always)]
    fn get_byte(self) -> Result<jbyte, Error> {
        self.and_then(|v| v.b()).map_err(jni_clear_ex)
    }
    #[inline(always)]
    fn get_char(self) -> Result<jchar, Error> {
        self.and_then(|v| v.c()).map_err(jni_clear_ex)
    }
    #[inline(always)]
    fn get_short(self) -> Result<jshort, Error> {
        self.and_then(|v| v.s()).map_err(jni_clear_ex)
    }
    #[inline(always)]
    fn get_int(self) -> Result<jint, Error> {
        self.and_then(|v| v.i()).map_err(jni_clear_ex)
    }
    #[inline(always)]
    fn get_long(self) -> Result<jlong, Error> {
        self.and_then(|v| v.j()).map_err(jni_clear_ex)
    }
    #[inline(always)]
    fn get_float(self) -> Result<jfloat, Error> {
        self.and_then(|v| v.f()).map_err(jni_clear_ex)
    }
    #[inline(always)]
    fn get_double(self) -> Result<jdouble, Error> {
        self.and_then(|v| v.d()).map_err(jni_clear_ex)
    }
}

/// Gets the value from the Java object; calls `jni_clear_ex()` for an error.
///
/// Functions defined here may return `Error::WrongJValueType`, which is probably misused.
pub trait JObjectGet<'a> {
    /// Checks if the object reference is null.
    fn is_null(&self) -> bool;

    /// Returns true if the object reference can be cast to the given type. Returns false for null.
    ///
    /// ```
    /// use jni_min_helper::*;
    /// let env = &mut jni_attach_vm().unwrap();
    /// let class_integer = env.find_class("java/lang/Integer").auto_local(env).unwrap();
    /// let integer = (3 as jni::sys::jint).create_jobject(env).unwrap();
    /// assert!(integer.is_class(class_integer.as_ref().into(), env).unwrap());
    /// ```
    fn is_class(&self, class: &JClass<'a>, env: &mut JNIEnv<'a>) -> Result<bool, Error>;

    /// Returns true if the object is a `java.lang.Number`.
    fn is_number(&self, env: &mut JNIEnv<'a>) -> Result<bool, Error>;

    /// Returns `Error::NullPtr(err_msg)` if the object reference is null.
    fn null_check(&self, err_msg: &'static str) -> Result<(), Error>;

    /// Does `null_check()`, returns `Error::WrongJValueType` if it is not a `java.lang.Number`.
    ///
    /// ```
    /// use jni_min_helper::*;
    /// use jni::{objects::JObject, sys::jint, errors::Error};
    ///
    /// let env = &mut jni_attach_vm().unwrap();
    /// let integer = (3 as jint).create_jobject(env).unwrap();
    /// let boolean = true.create_jobject(env).unwrap();
    /// assert!(integer.number_check(env).is_ok());
    /// assert!(matches!(boolean.number_check(env), Err(Error::WrongJValueType(_, _))));
    /// assert!(matches!(JObject::null().number_check(env), Err(Error::NullPtr(_))));
    /// ```
    fn number_check(&self, env: &mut JNIEnv<'a>) -> Result<(), Error>;

    /// Returns the binary name (internal form) of the object's class, or `Error::NullPtr`.
    ///
    /// ```
    /// use jni_min_helper::*;
    /// let env = &mut jni_attach_vm().unwrap();
    /// let boolean = true.create_jobject(env).unwrap();
    /// assert_eq!(boolean.get_class_name(env).unwrap(), "java/lang/Boolean");
    /// ```
    fn get_class_name(&self, env: &mut JNIEnv<'a>) -> Result<String, Error>;

    /// Returns the method name if it is a `java.lang.reflect.Method`.
    fn get_method_name(&self, env: &mut JNIEnv<'a>) -> Result<String, Error>;

    /// Returns the detail message string if it is a `java.lang.Throwable`.
    fn get_throwable_msg(&self, env: &mut JNIEnv<'a>) -> Result<String, Error>;

    /// Reads the string from `java.lang.String`. Returns an error if it's not a Java string.
    /// Returns `Ok(None)` if it's not a valid UTF-8 string.
    fn get_string(&self, env: &mut JNIEnv<'a>) -> Result<Option<String>, Error>;

    fn get_boolean(&self, env: &mut JNIEnv<'a>) -> Result<bool, Error>;
    fn get_char(&self, env: &mut JNIEnv<'a>) -> Result<jchar, Error>;

    fn get_byte(&self, env: &mut JNIEnv<'a>) -> Result<jbyte, Error>;
    fn get_short(&self, env: &mut JNIEnv<'a>) -> Result<jshort, Error>;
    fn get_int(&self, env: &mut JNIEnv<'a>) -> Result<jint, Error>;
    fn get_long(&self, env: &mut JNIEnv<'a>) -> Result<jlong, Error>;
    fn get_float(&self, env: &mut JNIEnv<'a>) -> Result<jfloat, Error>;
    fn get_double(&self, env: &mut JNIEnv<'a>) -> Result<jdouble, Error>;
}

impl<'a, T> JObjectGet<'a> for T
where
    T: AsRef<JObject<'a>>,
{
    #[inline(always)]
    fn is_null(&self) -> bool {
        // `env.is_same_object(self, JObject::null())` shouldn't be safer
        // with an invalid reference. Reference:
        // <https://docs.rs/jni/0.21.1/jni/objects/struct.JObject.html#method.from_raw>
        self.as_ref().as_raw().is_null()
    }

    #[inline(always)]
    fn is_class(&self, class: &JClass<'a>, env: &mut JNIEnv<'a>) -> Result<bool, Error> {
        if self.is_null() {
            return Ok(false);
        }
        env.is_instance_of(self, class)
    }

    #[inline(always)]
    fn is_number(&self, env: &mut JNIEnv<'a>) -> Result<bool, Error> {
        self.is_class(perf_store()?.abstract_number.as_obj().into(), env)
    }

    #[inline(always)]
    fn null_check(&self, err_msg: &'static str) -> Result<(), Error> {
        if !self.is_null() {
            Ok(())
        } else {
            Err(Error::NullPtr(err_msg))
        }
    }

    #[inline(always)]
    fn number_check(&self, env: &mut JNIEnv<'a>) -> Result<(), Error> {
        self.null_check("number_check")?;
        if !self.is_number(env)? {
            return Err(Error::WrongJValueType(
                "java.lang.Number",
                "check object class",
            ));
        }
        Ok(())
    }

    #[inline]
    fn get_class_name(&self, env: &mut JNIEnv<'a>) -> Result<String, Error> {
        self.null_check("get_class_name")?;
        unsafe {
            env.call_method_unchecked(
                env.get_object_class(self).auto_local(env)?,
                perf_store()?.get_class_name,
                ReturnType::Object,
                &[],
            )
        }
        .get_object(env)?
        .get_string(env)
        .map(|s| class_name_to_internal(&s.unwrap()))
    }

    #[inline]
    fn get_method_name(&self, env: &mut JNIEnv<'a>) -> Result<String, Error> {
        let perf_store = perf_store()?;
        if !self.is_class(perf_store.java_method.as_obj().into(), env)? {
            return Err(Error::WrongJValueType(
                "java.lang.reflect.Method",
                "check object class",
            ));
        }
        unsafe {
            env.call_method_unchecked(self, perf_store.get_method_name, ReturnType::Object, &[])
        }
        .get_object(env)?
        .get_string(env)
        .map(|s| s.unwrap())
    }

    #[inline]
    fn get_throwable_msg(&self, env: &mut JNIEnv<'a>) -> Result<String, Error> {
        let perf_store = perf_store()?;
        if !self.is_class(perf_store.java_throwable.as_obj().into(), env)? {
            return Err(Error::WrongJValueType(
                "java.lang.Throwable",
                "check object class",
            ));
        }
        unsafe {
            env.call_method_unchecked(self, perf_store.get_throwable_msg, ReturnType::Object, &[])
        }
        .get_object(env)?
        .get_string(env)
        .map(|s| s.unwrap())
    }

    #[inline(always)]
    fn get_string(&self, env: &mut JNIEnv<'a>) -> Result<Option<String>, Error> {
        if !self.is_class(perf_store()?.java_string.as_obj().into(), env)? {
            return Err(Error::WrongJValueType(
                "java.lang.String",
                "check object class",
            ));
        }
        let jstr =
            unsafe { env.get_string_unchecked(self.as_ref().into()) }.map_err(jni_clear_ex)?;
        Ok(jstr.to_str().map(|s| s.to_string()).ok())
    }

    #[inline(always)]
    fn get_boolean(&self, env: &mut JNIEnv<'a>) -> Result<bool, Error> {
        if !self.is_class(perf_store()?.wrapper_boolean.as_obj().into(), env)? {
            return Err(Error::WrongJValueType(
                "java.lang.Boolean",
                "check object class",
            ));
        }
        unsafe {
            env.call_method_unchecked(
                self,
                perf_store()?.get_boolean,
                ReturnType::Primitive(Primitive::Boolean),
                &[],
            )
        }
        .get_boolean()
    }
    #[inline(always)]
    fn get_char(&self, env: &mut JNIEnv<'a>) -> Result<jchar, Error> {
        if !self.is_class(perf_store()?.wrapper_character.as_obj().into(), env)? {
            return Err(Error::WrongJValueType(
                "java.lang.Character",
                "check object class",
            ));
        }
        unsafe {
            env.call_method_unchecked(
                self,
                perf_store()?.get_character,
                ReturnType::Primitive(Primitive::Char),
                &[],
            )
        }
        .get_char()
    }

    #[inline(always)]
    fn get_byte(&self, env: &mut JNIEnv<'a>) -> Result<jbyte, Error> {
        self.number_check(env)?;
        unsafe {
            env.call_method_unchecked(
                self,
                perf_store()?.get_byte,
                ReturnType::Primitive(Primitive::Byte),
                &[],
            )
        }
        .get_byte()
    }
    #[inline(always)]
    fn get_short(&self, env: &mut JNIEnv<'a>) -> Result<jshort, Error> {
        self.number_check(env)?;
        unsafe {
            env.call_method_unchecked(
                self,
                perf_store()?.get_short,
                ReturnType::Primitive(Primitive::Short),
                &[],
            )
        }
        .get_short()
    }
    #[inline(always)]
    fn get_int(&self, env: &mut JNIEnv<'a>) -> Result<jint, Error> {
        self.number_check(env)?;
        unsafe {
            env.call_method_unchecked(
                self,
                perf_store()?.get_integer,
                ReturnType::Primitive(Primitive::Int),
                &[],
            )
        }
        .get_int()
    }
    #[inline(always)]
    fn get_long(&self, env: &mut JNIEnv<'a>) -> Result<jlong, Error> {
        self.number_check(env)?;
        unsafe {
            env.call_method_unchecked(
                self,
                perf_store()?.get_long,
                ReturnType::Primitive(Primitive::Long),
                &[],
            )
        }
        .get_long()
    }
    #[inline(always)]
    fn get_float(&self, env: &mut JNIEnv<'a>) -> Result<jfloat, Error> {
        self.number_check(env)?;
        unsafe {
            env.call_method_unchecked(
                self,
                perf_store()?.get_float,
                ReturnType::Primitive(Primitive::Float),
                &[],
            )
        }
        .get_float()
    }
    #[inline(always)]
    fn get_double(&self, env: &mut JNIEnv<'a>) -> Result<jdouble, Error> {
        self.number_check(env)?;
        unsafe {
            env.call_method_unchecked(
                self,
                perf_store()?.get_double,
                ReturnType::Primitive(Primitive::Double),
                &[],
            )
        }
        .get_double()
    }
}

/// Creates the Java object for the Rust value.
///
/// ```
/// use jni_min_helper::*;
/// let env = &mut jni_attach_vm().unwrap();
///
/// assert_eq!("a×b".create_jobject(env).unwrap().get_string(env).unwrap(), Some("a×b".to_string()));
/// assert_eq!(false.create_jobject(env).unwrap().get_boolean(env).unwrap(), false);
/// assert_eq!(true.create_jobject(env).unwrap().get_boolean(env).unwrap(), true);
/// assert_eq!(0x000a_u16.create_jobject(env).unwrap().get_char(env).unwrap(), 0x000a_u16);
///
/// assert_eq!(0x39_i8.create_jobject(env).unwrap().get_byte(env).unwrap(), 0x39_i8);
/// assert_eq!(i16::MAX.create_jobject(env).unwrap().get_short(env).unwrap(), i16::MAX);
/// assert_eq!(i32::MAX.create_jobject(env).unwrap().get_int(env).unwrap(), i32::MAX);
/// assert_eq!(i64::MAX.create_jobject(env).unwrap().get_long(env).unwrap(), i64::MAX);
/// assert_eq!(3.14_f32.create_jobject(env).unwrap().get_float(env).unwrap(), 3.14_f32);
/// assert_eq!(3.14.create_jobject(env).unwrap().get_double(env).unwrap(), 3.14);
/// ```
pub trait JObjectCreate<'a> {
    fn create_jobject(&self, env: &mut JNIEnv<'a>) -> Result<AutoLocal<'a>, Error>;
}

impl<'a> JObjectCreate<'a> for str {
    fn create_jobject(&self, env: &mut JNIEnv<'a>) -> Result<AutoLocal<'a>, Error> {
        env.new_string(self).auto_local(env)
    }
}

impl<'a> JObjectCreate<'a> for bool {
    fn create_jobject(&self, env: &mut JNIEnv<'a>) -> Result<AutoLocal<'a>, Error> {
        let val = if *self { 1u8 } else { 0u8 };
        let perf_store = perf_store()?;
        unsafe {
            env.call_static_method_unchecked(
                &perf_store.wrapper_boolean,
                perf_store.value_of_boolean,
                ReturnType::Object,
                &[jvalue { z: val as jboolean }],
            )
        }
        .get_object(env)
    }
}

impl<'a> JObjectCreate<'a> for jchar {
    fn create_jobject(&self, env: &mut JNIEnv<'a>) -> Result<AutoLocal<'a>, Error> {
        let perf_store = perf_store()?;
        unsafe {
            env.call_static_method_unchecked(
                &perf_store.wrapper_character,
                perf_store.value_of_char,
                ReturnType::Object,
                &[jvalue { c: *self }],
            )
        }
        .get_object(env)
    }
}

impl<'a> JObjectCreate<'a> for jbyte {
    fn create_jobject(&self, env: &mut JNIEnv<'a>) -> Result<AutoLocal<'a>, Error> {
        let perf_store = perf_store()?;
        unsafe {
            env.call_static_method_unchecked(
                &perf_store.wrapper_byte,
                perf_store.value_of_byte,
                ReturnType::Object,
                &[jvalue { b: *self }],
            )
        }
        .get_object(env)
    }
}
impl<'a> JObjectCreate<'a> for jshort {
    fn create_jobject(&self, env: &mut JNIEnv<'a>) -> Result<AutoLocal<'a>, Error> {
        let perf_store = perf_store()?;
        unsafe {
            env.call_static_method_unchecked(
                &perf_store.wrapper_short,
                perf_store.value_of_short,
                ReturnType::Object,
                &[jvalue { s: *self }],
            )
        }
        .get_object(env)
    }
}
impl<'a> JObjectCreate<'a> for jint {
    fn create_jobject(&self, env: &mut JNIEnv<'a>) -> Result<AutoLocal<'a>, Error> {
        let perf_store = perf_store()?;
        unsafe {
            env.call_static_method_unchecked(
                &perf_store.wrapper_integer,
                perf_store.value_of_int,
                ReturnType::Object,
                &[jvalue { i: *self }],
            )
        }
        .get_object(env)
    }
}
impl<'a> JObjectCreate<'a> for jlong {
    fn create_jobject(&self, env: &mut JNIEnv<'a>) -> Result<AutoLocal<'a>, Error> {
        let perf_store = perf_store()?;
        unsafe {
            env.call_static_method_unchecked(
                &perf_store.wrapper_long,
                perf_store.value_of_long,
                ReturnType::Object,
                &[jvalue { j: *self }],
            )
        }
        .get_object(env)
    }
}
impl<'a> JObjectCreate<'a> for jfloat {
    fn create_jobject(&self, env: &mut JNIEnv<'a>) -> Result<AutoLocal<'a>, Error> {
        let perf_store = perf_store()?;
        unsafe {
            env.call_static_method_unchecked(
                &perf_store.wrapper_float,
                perf_store.value_of_float,
                ReturnType::Object,
                &[jvalue { f: *self }],
            )
        }
        .get_object(env)
    }
}
impl<'a> JObjectCreate<'a> for jdouble {
    fn create_jobject(&self, env: &mut JNIEnv<'a>) -> Result<AutoLocal<'a>, Error> {
        let perf_store = perf_store()?;
        unsafe {
            env.call_static_method_unchecked(
                &perf_store.wrapper_double,
                perf_store.value_of_double,
                ReturnType::Object,
                &[jvalue { d: *self }],
            )
        }
        .get_object(env)
    }
}

struct PerfStore {
    wrapper_boolean: GlobalRef,   // not Number
    wrapper_character: GlobalRef, // not Number
    abstract_number: GlobalRef,
    wrapper_byte: GlobalRef,
    wrapper_short: GlobalRef,
    wrapper_integer: GlobalRef,
    wrapper_long: GlobalRef,
    wrapper_float: GlobalRef,
    wrapper_double: GlobalRef,

    java_string: GlobalRef,
    java_method: GlobalRef,
    java_throwable: GlobalRef,

    get_boolean: JMethodID,
    get_character: JMethodID,
    get_byte: JMethodID,
    get_short: JMethodID,
    get_integer: JMethodID,
    get_long: JMethodID,
    get_float: JMethodID,
    get_double: JMethodID,

    value_of_boolean: JStaticMethodID,
    value_of_char: JStaticMethodID,
    value_of_byte: JStaticMethodID,
    value_of_short: JStaticMethodID,
    value_of_int: JStaticMethodID,
    value_of_long: JStaticMethodID,
    value_of_float: JStaticMethodID,
    value_of_double: JStaticMethodID,

    get_class_name: JMethodID,
    get_method_name: JMethodID,
    get_throwable_msg: JMethodID,
}

#[inline]
fn perf_store() -> Result<&'static PerfStore, Error> {
    static PERF_STORE: OnceLock<PerfStore> = OnceLock::new();
    if PERF_STORE.get().is_none() {
        let env = &mut jni_attach_vm()?;
        let wrapper_boolean = env.find_class("java/lang/Boolean").global_ref(env)?;
        let wrapper_character = env.find_class("java/lang/Character").global_ref(env)?;
        let abstract_number = env.find_class("java/lang/Number").global_ref(env)?;

        let _ = PERF_STORE.set(PerfStore {
            wrapper_boolean: wrapper_boolean.clone(),
            wrapper_character: wrapper_character.clone(),
            abstract_number: abstract_number.clone(),

            wrapper_byte: env.find_class("java/lang/Byte").global_ref(env)?,
            wrapper_short: env.find_class("java/lang/Short").global_ref(env)?,
            wrapper_integer: env.find_class("java/lang/Integer").global_ref(env)?,
            wrapper_long: env.find_class("java/lang/Long").global_ref(env)?,
            wrapper_float: env.find_class("java/lang/Float").global_ref(env)?,
            wrapper_double: env.find_class("java/lang/Double").global_ref(env)?,

            java_string: env.find_class("java/lang/String").global_ref(env)?,
            java_method: env.find_class("java/lang/reflect/Method").global_ref(env)?,
            java_throwable: env.find_class("java/lang/Throwable").global_ref(env)?,

            get_boolean: env.get_method_id(&wrapper_boolean, "booleanValue", "()Z")?,
            get_character: env.get_method_id(&wrapper_character, "charValue", "()C")?,

            get_byte: env.get_method_id(&abstract_number, "byteValue", "()B")?,
            get_short: env.get_method_id(&abstract_number, "shortValue", "()S")?,
            get_integer: env.get_method_id(&abstract_number, "intValue", "()I")?,
            get_long: env.get_method_id(&abstract_number, "longValue", "()J")?,
            get_float: env.get_method_id(&abstract_number, "floatValue", "()F")?,
            get_double: env.get_method_id(&abstract_number, "doubleValue", "()D")?,

            value_of_boolean: env.get_static_method_id(
                "java/lang/Boolean",
                "valueOf",
                "(Z)Ljava/lang/Boolean;",
            )?,
            value_of_char: env.get_static_method_id(
                "java/lang/Character",
                "valueOf",
                "(C)Ljava/lang/Character;",
            )?,
            value_of_byte: env.get_static_method_id(
                "java/lang/Byte",
                "valueOf",
                "(B)Ljava/lang/Byte;",
            )?,
            value_of_short: env.get_static_method_id(
                "java/lang/Short",
                "valueOf",
                "(S)Ljava/lang/Short;",
            )?,
            value_of_int: env.get_static_method_id(
                "java/lang/Integer",
                "valueOf",
                "(I)Ljava/lang/Integer;",
            )?,
            value_of_long: env.get_static_method_id(
                "java/lang/Long",
                "valueOf",
                "(J)Ljava/lang/Long;",
            )?,
            value_of_float: env.get_static_method_id(
                "java/lang/Float",
                "valueOf",
                "(F)Ljava/lang/Float;",
            )?,
            value_of_double: env.get_static_method_id(
                "java/lang/Double",
                "valueOf",
                "(D)Ljava/lang/Double;",
            )?,

            get_class_name: env.get_method_id(
                "java/lang/Class",
                "getName",
                "()Ljava/lang/String;",
            )?,
            get_method_name: env.get_method_id(
                "java/lang/reflect/Method",
                "getName",
                "()Ljava/lang/String;",
            )?,
            get_throwable_msg: env.get_method_id(
                "java/lang/Throwable",
                "getMessage",
                "()Ljava/lang/String;",
            )?,
        });
    }
    Ok(PERF_STORE.get().unwrap())
}

#[inline(always)]
pub(crate) fn class_name_to_internal(name: &str) -> String {
    name.replace(".", "/")
}

#[allow(unused)]
#[inline(always)]
pub(crate) fn class_name_to_java(name: &str) -> String {
    name.replace("/", ".")
}
