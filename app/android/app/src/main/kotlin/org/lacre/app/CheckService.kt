package org.lacre.app

import android.app.job.JobInfo
import android.app.job.JobParameters
import android.app.job.JobScheduler
import android.app.job.JobService
import android.content.ComponentName
import android.content.Context
import android.os.Handler
import android.os.Looper

/**
 * Comprobación periódica de mensajes (D-038). Con un intervalo elegido, el
 * proceso no sigue vivo en segundo plano: el sistema lo arranca cada
 * intervalo, [LacreApp] crea el motor y Dart arranca el nodo, que se conecta
 * por Tor, recoge lo que guardan los buzones y envía lo pendiente. Pasada
 * [VENTANA], si no hay pantalla, el proceso se cierra.
 */
class CheckService : JobService() {
    private val main = Handler(Looper.getMainLooper())
    private var fin: Runnable? = null

    override fun onStartJob(params: JobParameters): Boolean {
        val app = application as LacreApp
        app.comprobando = true
        fin = Runnable {
            app.comprobando = false
            jobFinished(params, false)
            app.apagarSiOculta()
        }.also { main.postDelayed(it, VENTANA) }
        return true
    }

    override fun onStopJob(params: JobParameters): Boolean {
        // El sistema corta la comprobación (sin red, fin de plazo): la
        // siguiente llega con el periodo.
        fin?.let { main.removeCallbacks(it) }
        (application as LacreApp).comprobando = false
        return false
    }

    companion object {
        private const val ID = 1
        /** Tiempo para arrancar Tor, publicar el servicio onion y recoger los buzones. */
        private const val VENTANA = 2 * 60 * 1000L

        /** Programa la comprobación cada `minutos` (0: ninguna, siempre conectado). */
        fun programar(context: Context, minutos: Int) {
            val js = context.getSystemService(JobScheduler::class.java)
            if (minutos <= 0) {
                js.cancel(ID)
                return
            }
            val periodo = minutos * 60 * 1000L
            val actual = js.getPendingJob(ID)
            if (actual != null && actual.intervalMillis == periodo) return
            val job = JobInfo.Builder(ID, ComponentName(context, CheckService::class.java))
                .setPeriodic(periodo)
                .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY)
                .setPersisted(true)
                .build()
            js.schedule(job)
        }
    }
}
