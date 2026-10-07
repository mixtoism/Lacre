package org.lacre.app

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.embedding.engine.FlutterEngineCache

/** La pantalla. Usa el motor de [LacreApp], que sigue vivo al cerrarla (D-032). */
class MainActivity : FlutterActivity() {
    override fun provideFlutterEngine(context: Context): FlutterEngine? =
        FlutterEngineCache.getInstance().get(LacreApp.ENGINE)

    override fun shouldDestroyEngineWithHost(): Boolean = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 1)
        }
        abrirChat(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        abrirChat(intent)
    }

    /** Al tocar el aviso de un mensaje, abre su chat. */
    private fun abrirChat(intent: Intent?) {
        val chat = intent?.getIntExtra(EXTRA_CHAT, -1) ?: -1
        if (chat < 0) return
        intent?.removeExtra(EXTRA_CHAT)
        (application as LacreApp).sistema.invokeMethod("abrirChat", chat)
    }

    companion object {
        const val EXTRA_CHAT = "chat"
    }
}
