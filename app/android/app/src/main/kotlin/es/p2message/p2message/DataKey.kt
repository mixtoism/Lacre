package es.p2message.p2message

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.io.File
import java.security.KeyStore
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Clave de datos para cifrar el estado en disco (D-033).
 *
 * Es aleatoria y se guarda envuelta (AES-GCM) con una clave del Android
 * Keystore, que no sale del hardware. El archivo envuelto va en
 * `noBackupFilesDir`: no entra en copias de seguridad.
 */
object DataKey {
    private const val ALIAS = "p2message-datos"
    private const val FILE = "clave-datos.bin"

    fun get(context: Context): ByteArray {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        val file = File(context.noBackupFilesDir, FILE)
        if (!ks.containsAlias(ALIAS)) {
            if (file.exists()) throw IllegalStateException("la clave del Keystore ha desaparecido")
            KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").apply {
                init(
                    KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                        .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                        .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                        .setKeySize(256)
                        .build()
                )
            }.generateKey()
        }
        val master = ks.getKey(ALIAS, null) as SecretKey
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        if (file.exists()) {
            val b = file.readBytes()
            cipher.init(Cipher.DECRYPT_MODE, master, GCMParameterSpec(128, b, 0, 12))
            return cipher.doFinal(b, 12, b.size - 12)
        }
        val key = ByteArray(32).also { SecureRandom().nextBytes(it) }
        cipher.init(Cipher.ENCRYPT_MODE, master)
        val tmp = File(context.noBackupFilesDir, "$FILE.tmp")
        tmp.writeBytes(cipher.iv + cipher.doFinal(key))
        if (!tmp.renameTo(file)) throw IllegalStateException("no se pudo guardar la clave de datos")
        return key
    }
}
