use crate::{jni_attach_vm, jni_clear_ex, AutoLocal, JObjectAutoLocal};
use jni::{
    descriptors::Desc,
    errors::Error,
    objects::{GlobalRef, JClass, JMethodID, JObject, JStaticMethodID, JValueOwned},
    signature::{
        Primitive,
        ReturnType::{Object as RetObj, Primitive as RetPrim},
    },
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

    #[doc(hidden)]
    fn sealer(_: private::Internal);
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

    fn sealer(_: private::Internal) {}
}

/// Gets the value from the Java object; calls `jni_clear_ex()` for an error.
///
/// Functions defined here may return `Error::WrongJValueType`, which is probably misused.
pub trait JObjectGet<'a> {
    /// Checks if the object reference is null.
    fn is_null(&self) -> bool;

    /// Returns `Error::NullPtr(err_msg)` if the JNI reference is null.
    fn null_check(&self, err_msg: &'static str) -> Result<&JObject<'a>, Error>;

    /// Does `null_check()` for an owned JNI reference.
    fn null_check_owned(self, err_msg: &'static str) -> Result<Self, Error>
    where
        Self: Sized;

    /// Does `null_check()`, returns `Error::WrongJValueType` if it is not an instance
    /// of the given class.
    fn class_check<'b, 'e>(
        &self,
        class: impl Desc<'e, JClass<'b>>,
        error_clue: &'static str,
        env: &mut JNIEnv<'e>,
    ) -> Result<&JObject<'a>, Error>;

    /// Does `class_check()` for an owned JNI reference.
    fn class_check_owned<'b, 'e>(
        self,
        class: impl Desc<'e, JClass<'b>>,
        error_clue: &'static str,
        env: &mut JNIEnv<'e>,
    ) -> Result<Self, Error>
    where
        Self: Sized;

    /// Does `class_check()` for `java.lang.Number`.
    ///
    /// ```
    /// use jni_min_helper::*;
    /// use jni::{objects::JObject, sys::jint, errors::Error};
    ///
    /// let env = &mut jni_attach_vm().unwrap();
    /// let integer = (3 as jint).new_jobject(env).unwrap();
    /// let boolean = true.new_jobject(env).unwrap();
    /// assert!(integer.number_check(env).is_ok());
    /// assert!(matches!(boolean.number_check(env), Err(Error::WrongJValueType(_, _))));
    /// assert!(matches!(JObject::null().number_check(env), Err(Error::NullPtr(_))));
    /// ```
    fn number_check<'e>(&self, env: &mut JNIEnv<'e>) -> Result<&JObject<'a>, Error>;

    /// Returns a referenced `jni::objects::JClass<'_>` JNI wrapper (unchecked).
    fn as_class(&self) -> &JClass<'a>;
    /// Returns a referenced `jni::objects::JClass<'_>` JNI wrapper. Returns an error if it
    /// is not a class object.
    fn as_class_checked(&self, env: &mut JNIEnv<'_>) -> Result<&JClass<'a>, Error>;

    /// Gets the value of an `java.lang.Boolean` wrapper.
    fn get_boolean(&self, env: &mut JNIEnv<'_>) -> Result<bool, Error>;
    /// Gets the value of an `java.lang.Character` wrapper.
    fn get_char(&self, env: &mut JNIEnv<'_>) -> Result<jchar, Error>;

    /// Gets the value of an `java.lang.Byte` wrapper.
    fn get_byte(&self, env: &mut JNIEnv<'_>) -> Result<jbyte, Error>;
    /// Gets the value of an `java.lang.Short` wrapper.
    fn get_short(&self, env: &mut JNIEnv<'_>) -> Result<jshort, Error>;
    /// Gets the value of an `java.lang.Integer` wrapper.
    fn get_int(&self, env: &mut JNIEnv<'_>) -> Result<jint, Error>;
    /// Gets the value of an `java.lang.Long` wrapper.
    fn get_long(&self, env: &mut JNIEnv<'_>) -> Result<jlong, Error>;
    /// Gets the value of an `java.lang.Float` wrapper.
    fn get_float(&self, env: &mut JNIEnv<'_>) -> Result<jfloat, Error>;
    /// Gets the value of an `java.lang.Double` wrapper.
    fn get_double(&self, env: &mut JNIEnv<'_>) -> Result<jdouble, Error>;

    /// Returns the binary name (internal form) of the object's class, or `Error::NullPtr`.
    ///
    /// ```
    /// use jni_min_helper::*;
    /// let env = &mut jni_attach_vm().unwrap();
    /// let boolean = true.new_jobject(env).unwrap();
    /// assert_eq!(boolean.get_class_name(env).unwrap(), "java/lang/Boolean");
    /// ```
    fn get_class_name(&self, env: &mut JNIEnv<'_>) -> Result<String, Error>;

    /// Returns the method name if it is a `java.lang.reflect.Method`.
    fn get_method_name(&self, env: &mut JNIEnv<'_>) -> Result<String, Error>;

    /// Returns the detail message string if it is a `java.lang.Throwable`.
    fn get_throwable_msg(&self, env: &mut JNIEnv<'_>) -> Result<String, Error>;

    /// Reads the string from `java.lang.String`. Returns an error if it is not a valid String.
    fn get_string(&self, env: &mut JNIEnv<'_>) -> Result<String, Error>;

    #[doc(hidden)]
    fn sealer(_: private::Internal);
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
    fn null_check(&self, err_msg: &'static str) -> Result<&JObject<'a>, Error> {
        if !self.is_null() {
            Ok(self.as_ref())
        } else {
            Err(Error::NullPtr(err_msg))
        }
    }

    #[inline(always)]
    fn null_check_owned(self, err_msg: &'static str) -> Result<Self, Error>
    where
        Self: Sized,
    {
        self.null_check(err_msg)?;
        Ok(self)
    }

    #[inline(always)]
    fn class_check<'b, 'e>(
        &self,
        class: impl Desc<'e, JClass<'b>>,
        error_clue: &'static str,
        env: &mut JNIEnv<'e>,
    ) -> Result<&JObject<'a>, Error> {
        self.null_check(error_clue)?;
        if env.is_instance_of(self, class)? {
            Ok(self.as_ref())
        } else {
            Err(Error::WrongJValueType("Object", error_clue))
        }
    }

    #[inline(always)]
    fn class_check_owned<'b, 'e>(
        self,
        class: impl Desc<'e, JClass<'b>>,
        error_clue: &'static str,
        env: &mut JNIEnv<'e>,
    ) -> Result<Self, Error>
    where
        Self: Sized,
    {
        self.class_check(class, error_clue, env)?;
        Ok(self)
    }

    #[inline(always)]
    fn number_check<'e>(&self, env: &mut JNIEnv<'e>) -> Result<&JObject<'a>, Error> {
        self.class_check(perf()?.abstract_number.as_class(), "number_check", env)
    }

    #[inline(always)]
    fn as_class_checked(&self, env: &mut JNIEnv<'_>) -> Result<&JClass<'a>, Error> {
        self.class_check(perf()?.java_class.as_class(), "as_class_checked", env)
            .map(|o| o.as_class())
    }

    #[inline(always)]
    fn as_class(&self) -> &JClass<'a> {
        self.as_ref().into()
    }

    #[inline(always)]
    fn get_boolean(&self, env: &mut JNIEnv<'_>) -> Result<bool, Error> {
        let perf = perf()?;
        self.class_check(perf.wrapper_boolean.as_class(), "get_boolean", env)?;
        unsafe {
            env.call_method_unchecked(self, perf.get_boolean, RetPrim(Primitive::Boolean), &[])
        }
        .get_boolean()
    }
    #[inline(always)]
    fn get_char(&self, env: &mut JNIEnv<'_>) -> Result<jchar, Error> {
        let perf = perf()?;
        self.class_check(perf.wrapper_character.as_class(), "get_char", env)?;
        unsafe {
            env.call_method_unchecked(self, perf.get_character, RetPrim(Primitive::Char), &[])
        }
        .get_char()
    }

    #[inline(always)]
    fn get_byte(&self, env: &mut JNIEnv<'_>) -> Result<jbyte, Error> {
        self.number_check(env)?;
        unsafe { env.call_method_unchecked(self, perf()?.get_byte, RetPrim(Primitive::Byte), &[]) }
            .get_byte()
    }
    #[inline(always)]
    fn get_short(&self, env: &mut JNIEnv<'_>) -> Result<jshort, Error> {
        self.number_check(env)?;
        unsafe {
            env.call_method_unchecked(self, perf()?.get_short, RetPrim(Primitive::Short), &[])
        }
        .get_short()
    }
    #[inline(always)]
    fn get_int(&self, env: &mut JNIEnv<'_>) -> Result<jint, Error> {
        self.number_check(env)?;
        unsafe {
            env.call_method_unchecked(self, perf()?.get_integer, RetPrim(Primitive::Int), &[])
        }
        .get_int()
    }
    #[inline(always)]
    fn get_long(&self, env: &mut JNIEnv<'_>) -> Result<jlong, Error> {
        self.number_check(env)?;
        unsafe { env.call_method_unchecked(self, perf()?.get_long, RetPrim(Primitive::Long), &[]) }
            .get_long()
    }
    #[inline(always)]
    fn get_float(&self, env: &mut JNIEnv<'_>) -> Result<jfloat, Error> {
        self.number_check(env)?;
        unsafe {
            env.call_method_unchecked(self, perf()?.get_float, RetPrim(Primitive::Float), &[])
        }
        .get_float()
    }
    #[inline(always)]
    fn get_double(&self, env: &mut JNIEnv<'_>) -> Result<jdouble, Error> {
        self.number_check(env)?;
        unsafe {
            env.call_method_unchecked(self, perf()?.get_double, RetPrim(Primitive::Double), &[])
        }
        .get_double()
    }

    #[inline]
    fn get_class_name(&self, env: &mut JNIEnv<'_>) -> Result<String, Error> {
        self.null_check("get_class_name")?;
        unsafe {
            env.call_method_unchecked(
                env.get_object_class(self).auto_local(env)?,
                perf()?.get_class_name,
                RetObj,
                &[],
            )
        }
        .get_object(env)?
        .get_string(env)
        .map(|s| class_name_to_internal(&s))
    }

    #[inline]
    fn get_method_name(&self, env: &mut JNIEnv<'_>) -> Result<String, Error> {
        let perf = perf()?;
        self.class_check(perf.java_method.as_class(), "get_method_name", env)?;
        unsafe { env.call_method_unchecked(self, perf.get_method_name, RetObj, &[]) }
            .get_object(env)?
            .get_string(env)
    }

    #[inline]
    fn get_throwable_msg(&self, env: &mut JNIEnv<'_>) -> Result<String, Error> {
        let perf = perf()?;
        self.class_check(perf.java_throwable.as_class(), "get_throwable_msg", env)?;
        unsafe { env.call_method_unchecked(self, perf.get_throwable_msg, RetObj, &[]) }
            .get_object(env)?
            .get_string(env)
    }

    #[inline(always)]
    fn get_string(&self, env: &mut JNIEnv<'_>) -> Result<String, Error> {
        self.class_check(perf()?.java_string.as_class(), "get_string", env)?;
        unsafe { env.get_string_unchecked(self.as_ref().into()) }
            .map_err(jni_clear_ex)?
            .to_str()
            .map(|s| s.to_string())
            // the last is unlikely to happen
            .map_err(|_| Error::WrongJValueType("String", "UTF-8 convertion failure"))
    }

    fn sealer(_: private::Internal) {}
}

/// Creates the Java object (wrapper) for the Rust value.
///
/// ```
/// use jni_min_helper::*;
/// let env = &mut jni_attach_vm().unwrap();
///
/// assert_eq!("a×b".new_jobject(env).unwrap().get_string(env).unwrap(), "a×b");
/// assert_eq!(false.new_jobject(env).unwrap().get_boolean(env).unwrap(), false);
/// assert_eq!(true.new_jobject(env).unwrap().get_boolean(env).unwrap(), true);
/// assert_eq!(0x000a_u16.new_jobject(env).unwrap().get_char(env).unwrap(), 0x000a_u16);
///
/// assert_eq!(0x39_i8.new_jobject(env).unwrap().get_byte(env).unwrap(), 0x39_i8);
/// assert_eq!(i16::MAX.new_jobject(env).unwrap().get_short(env).unwrap(), i16::MAX);
/// assert_eq!(i32::MAX.new_jobject(env).unwrap().get_int(env).unwrap(), i32::MAX);
/// assert_eq!(i64::MAX.new_jobject(env).unwrap().get_long(env).unwrap(), i64::MAX);
/// assert_eq!(3.14_f32.new_jobject(env).unwrap().get_float(env).unwrap(), 3.14_f32);
/// assert_eq!(3.14.new_jobject(env).unwrap().get_double(env).unwrap(), 3.14);
/// ```
pub trait JObjectNew<'a> {
    fn new_jobject(&self, env: &mut JNIEnv<'a>) -> Result<AutoLocal<'a>, Error>;
}

impl<'a> JObjectNew<'a> for str {
    fn new_jobject(&self, env: &mut JNIEnv<'a>) -> Result<AutoLocal<'a>, Error> {
        env.new_string(self).auto_local(env)
    }
}

impl<'a> JObjectNew<'a> for bool {
    fn new_jobject(&self, env: &mut JNIEnv<'a>) -> Result<AutoLocal<'a>, Error> {
        let val = if *self { 1u8 } else { 0u8 };
        let perf = perf()?;
        unsafe {
            env.call_static_method_unchecked(
                &perf.wrapper_boolean,
                perf.value_of_boolean,
                RetObj,
                &[jvalue { z: val as jboolean }],
            )
        }
        .get_object(env)
    }
}

impl<'a> JObjectNew<'a> for jchar {
    fn new_jobject(&self, env: &mut JNIEnv<'a>) -> Result<AutoLocal<'a>, Error> {
        let perf = perf()?;
        unsafe {
            env.call_static_method_unchecked(
                &perf.wrapper_character,
                perf.value_of_char,
                RetObj,
                &[jvalue { c: *self }],
            )
        }
        .get_object(env)
    }
}

impl<'a> JObjectNew<'a> for jbyte {
    fn new_jobject(&self, env: &mut JNIEnv<'a>) -> Result<AutoLocal<'a>, Error> {
        let perf = perf()?;
        unsafe {
            env.call_static_method_unchecked(
                &perf.wrapper_byte,
                perf.value_of_byte,
                RetObj,
                &[jvalue { b: *self }],
            )
        }
        .get_object(env)
    }
}
impl<'a> JObjectNew<'a> for jshort {
    fn new_jobject(&self, env: &mut JNIEnv<'a>) -> Result<AutoLocal<'a>, Error> {
        let perf = perf()?;
        unsafe {
            env.call_static_method_unchecked(
                &perf.wrapper_short,
                perf.value_of_short,
                RetObj,
                &[jvalue { s: *self }],
            )
        }
        .get_object(env)
    }
}
impl<'a> JObjectNew<'a> for jint {
    fn new_jobject(&self, env: &mut JNIEnv<'a>) -> Result<AutoLocal<'a>, Error> {
        let perf = perf()?;
        unsafe {
            env.call_static_method_unchecked(
                &perf.wrapper_integer,
                perf.value_of_int,
                RetObj,
                &[jvalue { i: *self }],
            )
        }
        .get_object(env)
    }
}
impl<'a> JObjectNew<'a> for jlong {
    fn new_jobject(&self, env: &mut JNIEnv<'a>) -> Result<AutoLocal<'a>, Error> {
        let perf = perf()?;
        unsafe {
            env.call_static_method_unchecked(
                &perf.wrapper_long,
                perf.value_of_long,
                RetObj,
                &[jvalue { j: *self }],
            )
        }
        .get_object(env)
    }
}
impl<'a> JObjectNew<'a> for jfloat {
    fn new_jobject(&self, env: &mut JNIEnv<'a>) -> Result<AutoLocal<'a>, Error> {
        let perf = perf()?;
        unsafe {
            env.call_static_method_unchecked(
                &perf.wrapper_float,
                perf.value_of_float,
                RetObj,
                &[jvalue { f: *self }],
            )
        }
        .get_object(env)
    }
}
impl<'a> JObjectNew<'a> for jdouble {
    fn new_jobject(&self, env: &mut JNIEnv<'a>) -> Result<AutoLocal<'a>, Error> {
        let perf = perf()?;
        unsafe {
            env.call_static_method_unchecked(
                &perf.wrapper_double,
                perf.value_of_double,
                RetObj,
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
    java_class: GlobalRef,
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
fn perf() -> Result<&'static PerfStore, Error> {
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
            java_class: env.find_class("java/lang/Class").global_ref(env)?,
            java_method: env.find_class("java/lang/reflect/Method").global_ref(env)?,
            java_throwable: env.find_class("java/lang/Throwable").global_ref(env)?,

            get_boolean: env
                .get_method_id(&wrapper_boolean, "booleanValue", "()Z")
                .map_err(jni_clear_ex)?,
            get_character: env
                .get_method_id(&wrapper_character, "charValue", "()C")
                .map_err(jni_clear_ex)?,

            get_byte: env
                .get_method_id(&abstract_number, "byteValue", "()B")
                .map_err(jni_clear_ex)?,
            get_short: env
                .get_method_id(&abstract_number, "shortValue", "()S")
                .map_err(jni_clear_ex)?,
            get_integer: env
                .get_method_id(&abstract_number, "intValue", "()I")
                .map_err(jni_clear_ex)?,
            get_long: env
                .get_method_id(&abstract_number, "longValue", "()J")
                .map_err(jni_clear_ex)?,
            get_float: env
                .get_method_id(&abstract_number, "floatValue", "()F")
                .map_err(jni_clear_ex)?,
            get_double: env
                .get_method_id(&abstract_number, "doubleValue", "()D")
                .map_err(jni_clear_ex)?,

            value_of_boolean: env
                .get_static_method_id("java/lang/Boolean", "valueOf", "(Z)Ljava/lang/Boolean;")
                .map_err(jni_clear_ex)?,
            value_of_char: env
                .get_static_method_id("java/lang/Character", "valueOf", "(C)Ljava/lang/Character;")
                .map_err(jni_clear_ex)?,
            value_of_byte: env
                .get_static_method_id("java/lang/Byte", "valueOf", "(B)Ljava/lang/Byte;")
                .map_err(jni_clear_ex)?,
            value_of_short: env
                .get_static_method_id("java/lang/Short", "valueOf", "(S)Ljava/lang/Short;")
                .map_err(jni_clear_ex)?,
            value_of_int: env
                .get_static_method_id("java/lang/Integer", "valueOf", "(I)Ljava/lang/Integer;")
                .map_err(jni_clear_ex)?,
            value_of_long: env
                .get_static_method_id("java/lang/Long", "valueOf", "(J)Ljava/lang/Long;")
                .map_err(jni_clear_ex)?,
            value_of_float: env
                .get_static_method_id("java/lang/Float", "valueOf", "(F)Ljava/lang/Float;")
                .map_err(jni_clear_ex)?,
            value_of_double: env
                .get_static_method_id("java/lang/Double", "valueOf", "(D)Ljava/lang/Double;")
                .map_err(jni_clear_ex)?,

            get_class_name: env
                .get_method_id("java/lang/Class", "getName", "()Ljava/lang/String;")
                .map_err(jni_clear_ex)?,
            get_method_name: env
                .get_method_id(
                    "java/lang/reflect/Method",
                    "getName",
                    "()Ljava/lang/String;",
                )
                .map_err(jni_clear_ex)?,
            get_throwable_msg: env
                .get_method_id("java/lang/Throwable", "getMessage", "()Ljava/lang/String;")
                .map_err(jni_clear_ex)?,
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

mod private {
    /// Used as a parameter of the hidden function in sealed traits.
    #[derive(Debug)]
    pub struct Internal;
}
