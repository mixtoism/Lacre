package org.lacre.app

import android.app.Activity
import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.Uri
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.PowerManager
import android.os.Process
import android.provider.Settings
import android.webkit.MimeTypeMap
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.FileProvider
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.embedding.engine.FlutterEngineCache
import io.flutter.embedding.engine.dart.DartExecutor
import io.flutter.plugin.common.MethodChannel
import java.io.File

/**
 * El nodo vive en el proceso, no en la pantalla (D-032): el motor de Flutter
 * se crea aquí y sobrevive a la actividad. Mientras el servicio en primer
 * plano ([NodeService]) mantiene vivo el proceso, el nodo sigue recibiendo
 * mensajes con la app cerrada. Si la persona elige comprobar cada cierto
 * tiempo (D-038), el proceso se cierra al salir de la app y [CheckService]
 * lo vuelve a arrancar en cada comprobación.
 */
class LacreApp : Application() {
    private var multicastLock: WifiManager.MulticastLock? = null
    /** La red por defecto es wifi (o cable): mDNS tiene sentido (D-034). */
    @Volatile private var wifi = true
    /** Actividades visibles: con alguna, la app está en primer plano. */
    private var visibles = 0

    /** Actividad en primer plano, para FLAG_SECURE (D-048). */
    private var actividad: java.lang.ref.WeakReference<Activity>? = null
    /** Hay una comprobación periódica en curso ([CheckService]). */
    var comprobando = false
    private val main = Handler(Looper.getMainLooper())
    private val apagar = Runnable { apagarSiOculta() }
    lateinit var engine: FlutterEngine
        private set
    lateinit var sistema: MethodChannel
        private set

    private external fun initAndroidContext(context: Context)

    override fun onCreate() {
        super.onCreate()
        // Antes de que Dart arranque el nodo: el núcleo en Rust necesita el
        // Context de Android (configuración DNS del sistema).
        System.loadLibrary("lacre_api")
        initAndroidContext(this)
        // Sin este lock no llegan los paquetes de mDNS (descubrimiento en la
        // red local). Dart lo pide solo mientras mDNS está activo (D-034).
        val wm = getSystemService(Context.WIFI_SERVICE) as WifiManager
        multicastLock = wm.createMulticastLock("lacre-mdns").apply {
            setReferenceCounted(false)
            acquire()
        }
        crearCanales()
        vigilarRed()
        vigilarPantalla()
        CheckService.programar(this, intervalo())

        engine = FlutterEngine(this)
        sistema = MethodChannel(engine.dartExecutor.binaryMessenger, "lacre/sistema")
        sistema.setMethodCallHandler { call, result ->
            try {
                when (call.method) {
                    "claveDatos" -> result.success(DataKey.get(this))
                    "servicio" -> {
                        // Arrancado por una comprobación, sin pantalla: el
                        // proceso vive lo que dure ella, sin servicio.
                        if (intervalo() == 0 || visibles > 0) NodeService.start(this)
                        result.success(true)
                    }
                    "intervalo" -> result.success(intervalo())
                    "fijarIntervalo" -> {
                        fijarIntervalo(call.arguments as Int)
                        result.success(true)
                    }
                    "aviso" -> {
                        aviso(call.argument<Int>("id")!!, call.argument<String>("titulo")!!,
                            call.argument<String>("texto")!!)
                        result.success(true)
                    }
                    "quitarAviso" -> {
                        NotificationManagerCompat.from(this).cancel(AVISO_BASE + call.argument<Int>("id")!!)
                        result.success(true)
                    }
                    "bateriaLibre" -> result.success(bateriaLibre())
                    "wifi" -> result.success(wifi)
                    "multicast" -> {
                        val lock = multicastLock
                        if (call.arguments as Boolean) lock?.acquire() else if (lock?.isHeld == true) lock.release()
                        result.success(true)
                    }
                    "pedirBateria" -> {
                        pedirBateria()
                        result.success(true)
                    }
                    "abrir" -> result.success(abrir(File(call.argument<String>("path")!!)))
                    "pantallaSegura" -> {
                        // Sin capturas ni vista previa en recientes mientras
                        // se ve la frase de recuperación (D-048).
                        val w = actividad?.get()?.window
                        val flag = android.view.WindowManager.LayoutParams.FLAG_SECURE
                        if (call.arguments as Boolean) w?.addFlags(flag) else w?.clearFlags(flag)
                        result.success(w != null)
                    }
                    else -> result.notImplemented()
                }
            } catch (e: Exception) {
                result.error(call.method, e.message, null)
            }
        }
        engine.dartExecutor.executeDartEntrypoint(DartExecutor.DartEntrypoint.createDefault())
        FlutterEngineCache.getInstance().put(ENGINE, engine)
    }

    /** Minutos entre comprobaciones de mensajes; 0: siempre conectado (D-038). */
    private fun intervalo(): Int =
        getSharedPreferences(PREFS, Context.MODE_PRIVATE).getInt(INTERVALO, 0)

    private fun fijarIntervalo(minutos: Int) {
        getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putInt(INTERVALO, minutos).apply()
        CheckService.programar(this, minutos)
        if (minutos == 0) NodeService.start(this)
    }

    /**
     * Con intervalo, al salir de la app el nodo sigue [GRACIA] (para terminar
     * de enviar) con el servicio en primer plano, y después se cierra el
     * proceso. El servicio se arranca al abrir la pantalla: desde segundo
     * plano Android no deja.
     */
    private fun vigilarPantalla() {
        registerActivityLifecycleCallbacks(object : ActivityLifecycleCallbacks {
            override fun onActivityStarted(activity: Activity) {
                visibles++
                main.removeCallbacks(apagar)
                NodeService.start(this@LacreApp)
            }
            override fun onActivityStopped(activity: Activity) {
                visibles--
                if (visibles == 0 && intervalo() > 0) main.postDelayed(apagar, GRACIA)
            }
            override fun onActivityCreated(activity: Activity, state: Bundle?) {}
            override fun onActivityResumed(activity: Activity) {
                actividad = java.lang.ref.WeakReference(activity)
            }
            override fun onActivityPaused(activity: Activity) {}
            override fun onActivitySaveInstanceState(activity: Activity, state: Bundle) {}
            override fun onActivityDestroyed(activity: Activity) {}
        })
    }

    /** Con intervalo, sin pantalla ni comprobación en curso, cierra el proceso (y el nodo). */
    fun apagarSiOculta() {
        if (intervalo() == 0 || visibles > 0 || comprobando) return
        // Primero el servicio: si no, START_STICKY volvería a arrancarlo.
        stopService(Intent(this, NodeService::class.java))
        // Margen para que JobScheduler anote el fin de la comprobación.
        main.postDelayed({
            if (visibles == 0 && !comprobando) Process.killProcess(Process.myPid())
        }, 2000)
    }

    /** Avisa a Dart cuando la red por defecto pasa de wifi a datos o al revés. */
    private fun vigilarRed() {
        val cm = getSystemService(ConnectivityManager::class.java)
        cm.getNetworkCapabilities(cm.activeNetwork)?.let { wifi = esWifi(it) }
        val main = Handler(Looper.getMainLooper())
        cm.registerDefaultNetworkCallback(object : ConnectivityManager.NetworkCallback() {
            override fun onCapabilitiesChanged(network: Network, caps: NetworkCapabilities) = cambia(esWifi(caps))
            override fun onLost(network: Network) = cambia(false)
            private fun cambia(w: Boolean) {
                if (w == wifi) return
                wifi = w
                main.post { sistema.invokeMethod("red", w) }
            }
        })
    }

    private fun esWifi(caps: NetworkCapabilities) =
        caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) ||
            caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET)

    private fun crearCanales() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(
            NotificationChannel(CANAL_CONEXION, getString(R.string.canal_conexion), NotificationManager.IMPORTANCE_MIN).apply {
                description = getString(R.string.canal_conexion_desc)
                setShowBadge(false)
            }
        )
        nm.createNotificationChannel(
            NotificationChannel(CANAL_MENSAJES, getString(R.string.canal_mensajes), NotificationManager.IMPORTANCE_HIGH).apply {
                description = getString(R.string.canal_mensajes_desc)
            }
        )
    }

    /** Aviso de un mensaje nuevo. En la pantalla de bloqueo no se ve el texto. */
    private fun aviso(chat: Int, titulo: String, texto: String) {
        val abrir = Intent(this, MainActivity::class.java)
            .setFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
            .putExtra(MainActivity.EXTRA_CHAT, chat)
        val pi = PendingIntent.getActivity(this, chat, abrir,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        val publico = NotificationCompat.Builder(this, CANAL_MENSAJES)
            .setSmallIcon(R.drawable.ic_aviso)
            .setContentTitle("Lacre")
            .setContentText(getString(R.string.mensaje_nuevo))
            .build()
        val n = NotificationCompat.Builder(this, CANAL_MENSAJES)
            .setSmallIcon(R.drawable.ic_aviso)
            .setContentTitle(titulo)
            .setContentText(texto)
            .setStyle(NotificationCompat.BigTextStyle().bigText(texto))
            .setCategory(NotificationCompat.CATEGORY_MESSAGE)
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
            .setPublicVersion(publico)
            .setAutoCancel(true)
            .setContentIntent(pi)
            .build()
        try {
            NotificationManagerCompat.from(this).notify(AVISO_BASE + chat, n)
        } catch (e: SecurityException) {
            // Sin permiso de notificaciones: no hay aviso.
        }
    }

    /** La app está exenta de la optimización de batería (red con la pantalla apagada). */
    private fun bateriaLibre(): Boolean {
        val pm = getSystemService(Context.POWER_SERVICE) as PowerManager
        return pm.isIgnoringBatteryOptimizations(packageName)
    }

    private fun pedirBateria() {
        val intent = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS)
            .setData(Uri.parse("package:$packageName"))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        startActivity(intent)
    }

    /** Abre un archivo de un chat con la app que elija la persona. */
    private fun abrir(file: File): Boolean {
        val uri = FileProvider.getUriForFile(this, "$packageName.archivos", file)
        val ext = file.extension.lowercase()
        val mime = MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext) ?: "application/octet-stream"
        val intent = Intent(Intent.ACTION_VIEW)
            .setDataAndType(uri, mime)
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
        return try {
            startActivity(intent)
            true
        } catch (e: ActivityNotFoundException) {
            false
        }
    }

    companion object {
        const val ENGINE = "principal"
        const val CANAL_CONEXION = "conexion"
        const val CANAL_MENSAJES = "mensajes"
        /** Los avisos de mensajes van de AVISO_BASE + índice del chat. */
        const val AVISO_BASE = 1000
        private const val PREFS = "lacre"
        private const val INTERVALO = "intervalo"
        /** Tras salir de la app, cuánto sigue conectado el nodo con intervalo. */
        private const val GRACIA = 60 * 1000L
    }
}
