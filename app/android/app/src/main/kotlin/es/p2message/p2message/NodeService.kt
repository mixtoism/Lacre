package es.p2message.p2message

import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.os.Process
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat

/**
 * Servicio en primer plano que mantiene vivo el proceso, y con él el nodo,
 * con la app cerrada (D-032). No hace nada más: el nodo corre en el motor de
 * Flutter que crea [P2mApp].
 */
class NodeService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACCION_CERRAR) {
            // El nodo vive en el proceso: cerrar es terminarlo.
            ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
            stopSelf()
            Process.killProcess(Process.myPid())
            return START_NOT_STICKY
        }
        val abrir = PendingIntent.getActivity(this, 0,
            Intent(this, MainActivity::class.java).setFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
            PendingIntent.FLAG_IMMUTABLE)
        val cerrar = PendingIntent.getService(this, 1,
            Intent(this, NodeService::class.java).setAction(ACCION_CERRAR),
            PendingIntent.FLAG_IMMUTABLE)
        val n = NotificationCompat.Builder(this, P2mApp.CANAL_CONEXION)
            .setSmallIcon(R.drawable.ic_aviso)
            .setContentTitle("p2message")
            .setContentText(getString(R.string.servicio_texto))
            .setOngoing(true)
            .setPriority(NotificationCompat.PRIORITY_MIN)
            .setContentIntent(abrir)
            .addAction(0, getString(R.string.servicio_cerrar), cerrar)
            .build()
        val type = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
            ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE else 0
        ServiceCompat.startForeground(this, ID, n, type)
        // Si el sistema mata el proceso, que lo vuelva a arrancar: P2mApp
        // crea el motor y Dart arranca el nodo.
        return START_STICKY
    }

    companion object {
        private const val ID = 1
        private const val ACCION_CERRAR = "es.p2message.CERRAR"

        fun start(context: Context) {
            ContextCompat.startForegroundService(context, Intent(context, NodeService::class.java))
        }
    }
}
