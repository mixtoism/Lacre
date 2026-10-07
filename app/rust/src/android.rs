//! Inicialización específica de Android.
//!
//! Algunas dependencias (el resolvedor DNS de libp2p, hickory) leen la
//! configuración del sistema por JNI y necesitan la JVM y el `Context` de la
//! app a través de `ndk-context`. `LacreApp` (la `Application`) la llama antes
//! de arrancar Flutter.

use std::ffi::c_void;
use std::sync::Once;

use jni_sys::{JNIEnv, JavaVM, jobject};

static INIT: Once = Once::new();

/// `LacreApp.initAndroidContext(context)`.
#[no_mangle]
pub unsafe extern "system" fn Java_org_lacre_app_LacreApp_initAndroidContext(
    env: *mut JNIEnv,
    _this: jobject,
    context: jobject,
) {
    // La actividad puede recrearse en el mismo proceso; ndk-context solo
    // admite una inicialización.
    INIT.call_once(|| unsafe {
        let mut vm: *mut JavaVM = std::ptr::null_mut();
        if ((**env).v1_1.GetJavaVM)(env, &mut vm) != 0 {
            return;
        }
        // Referencia global: el Context de la aplicación vive lo que el proceso.
        let context = ((**env).v1_1.NewGlobalRef)(env, context);
        ndk_context::initialize_android_context(vm as *mut c_void, context as *mut c_void);
    });
}
