// ignore: unused_import
import 'package:intl/intl.dart' as intl;

import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for Spanish Castilian (`es`).
class AppLocalizationsEs extends AppLocalizations {
  AppLocalizationsEs([String locale = 'es']) : super(locale);

  @override
  String startError(String error) {
    return 'No se pudo arrancar el nodo:\n\n$error';
  }

  @override
  String copied(String what) {
    return '$what copiado';
  }

  @override
  String get copy => 'Copiar';

  @override
  String get cancel => 'Cancelar';

  @override
  String get noAppToOpen => 'Ninguna app puede abrir este archivo';

  @override
  String get cantOpenFile => 'No se pudo abrir el archivo';

  @override
  String get onboardingTagline =>
      'Mensajería cifrada de extremo a extremo, sin servidores. Tus claves no salen de tus dispositivos.';

  @override
  String get newIdentity => 'Identidad nueva';

  @override
  String get linkDevice => 'Vincular dispositivo';

  @override
  String get name => 'Nombre';

  @override
  String get nameHelper =>
      'Minúsculas, números y _. Se le añadirá un sufijo: nombre#xxxxxxxx';

  @override
  String get createIdentity => 'Crear identidad';

  @override
  String get linkInstructions =>
      'En el dispositivo principal abre Perfil → Dispositivos → Vincular y escanea este QR o pega el código. Esta pantalla cambiará sola cuando se complete.';

  @override
  String get code => 'Código';

  @override
  String get waitingPrimary => 'Esperando al dispositivo principal…';

  @override
  String get noCameraPermission => 'Sin permiso para usar la cámara';

  @override
  String cantOpenCamera(String code) {
    return 'No se pudo abrir la cámara: $code';
  }

  @override
  String get chat => 'Chat';

  @override
  String get closeSearch => 'Cerrar búsqueda';

  @override
  String get searchAllChats => 'Buscar en todos los chats';

  @override
  String get clear => 'Borrar';

  @override
  String get search => 'Buscar';

  @override
  String get newChat => 'Nuevo chat';

  @override
  String get noChatsYet => 'Aún no tienes chats';

  @override
  String get noChatsHint =>
      'Pulsa «Nuevo chat» y escribe el handle de alguien o escanea su QR.';

  @override
  String get chatRequests => 'Solicitudes de chat';

  @override
  String get chatRequestsHint =>
      'De personas con las que aún no hablas. Hasta que aceptes no se muestran sus imágenes ni se descargan sus archivos.';

  @override
  String get chats => 'Chats';

  @override
  String get sticker => 'Sticker';

  @override
  String get you => 'Tú';

  @override
  String get request => 'Solicitud';

  @override
  String get notInChat => 'Ya no participas en este chat';

  @override
  String get groupNoMessages => 'Grupo · sin mensajes';

  @override
  String get noMessages => 'Sin mensajes';

  @override
  String get noResults => 'Sin resultados';

  @override
  String messageCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count mensajes',
      one: '1 mensaje',
    );
    return '$_temp0';
  }

  @override
  String peersConnected(int count) {
    return '$count peers conectados';
  }

  @override
  String get onionPublished => 'Servicio onion publicado';

  @override
  String get onionPublishing => 'Servicio onion: publicando…';

  @override
  String get reachableByRelay => 'Alcanzable por relay';

  @override
  String get offline => 'sin red';

  @override
  String get scanProfileQr => 'Escanea el QR de su perfil';

  @override
  String get participants => 'Participantes';

  @override
  String get participantsHelper =>
      'Handles (nombre#sufijo) o códigos QR (p2m:…),\nseparados por espacios o líneas';

  @override
  String get scanQr => 'Escanear QR';

  @override
  String get groupTitleOptional => 'Título del grupo (opcional)';

  @override
  String get searchingNetwork =>
      'Buscando en la red… (por Tor puede tardar un minuto)';

  @override
  String get create => 'Crear';

  @override
  String get deleteChatBodyDirect =>
      'Se borran sus mensajes de este dispositivo. Si alguien vuelve a escribir, el chat reaparecerá vacío.';

  @override
  String get deleteChatBodyManager =>
      'Se borran sus mensajes de este dispositivo. Como el grupo lo gestiona este dispositivo, sigues en él: los demás pueden seguir escribiendo y el chat reaparecerá vacío.';

  @override
  String get deleteChatBodyLeave =>
      'Saldrás del grupo en todos tus dispositivos y se borrarán sus mensajes de este. Solo volverás si alguien te añade de nuevo.';

  @override
  String deleteChatTitle(String name) {
    return '¿Borrar «$name»?';
  }

  @override
  String get delete => 'Borrar';

  @override
  String get leaveAndDelete => 'Salir y borrar';

  @override
  String blockTitle(String name) {
    return '¿Bloquear a $name?';
  }

  @override
  String get blockBody =>
      'No verás sus mensajes ni sus invitaciones a grupos, y no podrá ver tu foto de perfil. No se le avisa. Puedes desbloquearle desde tu perfil.';

  @override
  String get block => 'Bloquear';

  @override
  String blockPerson(String name) {
    return 'Bloquear a $name';
  }

  @override
  String get acceptRequest => 'Aceptar solicitud';

  @override
  String get deleteChat => 'Borrar chat';

  @override
  String removeTitle(String handle) {
    return '¿Quitar a $handle?';
  }

  @override
  String get removeBody =>
      'Saldrán del chat todos sus dispositivos y no recibirá los mensajes nuevos.';

  @override
  String get remove => 'Quitar';

  @override
  String get members => 'Miembros';

  @override
  String get add => 'Añadir';

  @override
  String get onlyCreatorEdits =>
      'Solo quien creó el chat puede añadir o quitar personas.';

  @override
  String removePerson(String handle) {
    return 'Quitar a $handle';
  }

  @override
  String get addPeople => 'Añadir personas';

  @override
  String get people => 'Personas';

  @override
  String get peopleHelper => 'Handles (nombre#sufijo) o códigos QR (p2m:…)';

  @override
  String fileTooBig(String size, String max) {
    return 'El archivo ocupa $size (máximo $max)';
  }

  @override
  String deliveryFailed(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: 'No se pudo entregar a $count dispositivos; se reintentará',
      one: 'No se pudo entregar a 1 dispositivo; se reintentará',
    );
    return '$_temp0';
  }

  @override
  String get notInChatDot => 'Ya no participas en este chat.';

  @override
  String deviceCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count dispositivos',
      one: '1 dispositivo',
    );
    return '$_temp0';
  }

  @override
  String get searchInChat => 'Buscar en el chat';

  @override
  String get more => 'Más';

  @override
  String get searchThisChat => 'Buscar en este chat';

  @override
  String get previousOlder => 'Anterior (más antiguo)';

  @override
  String get nextNewer => 'Siguiente (más reciente)';

  @override
  String get emojiAndStickers => 'Emoji y stickers';

  @override
  String get attachFile => 'Adjuntar archivo';

  @override
  String get message => 'Mensaje';

  @override
  String get send => 'Enviar';

  @override
  String wantsToChat(String name) {
    return '$name quiere chatear contigo';
  }

  @override
  String invitedTo(String name) {
    return 'Te han invitado a «$name»';
  }

  @override
  String get requestBannerBody =>
      'Aún no habláis. Hasta que aceptes no se muestran sus stickers ni se descargan sus archivos, y no ve tu foto de perfil.';

  @override
  String get accept => 'Aceptar';

  @override
  String get alreadyInStickers => 'Ya está en tus stickers';

  @override
  String get saveToStickers => 'Guardar en mis stickers';

  @override
  String get savedToStickers => 'Guardado en tus stickers';

  @override
  String get stickerHidden => 'Sticker oculto hasta que aceptes';

  @override
  String get open => 'Abrir';

  @override
  String get acceptToDownload => 'Acepta la solicitud para descargarlo';

  @override
  String get download => 'Descargar';

  @override
  String get images => 'Imágenes';

  @override
  String get defaultDeviceLabel => 'portátil';

  @override
  String get linkDialogScan =>
      'En el dispositivo nuevo elige «Vincular dispositivo» y escanea su QR o pega aquí su código.';

  @override
  String get linkDialogPaste =>
      'En el dispositivo nuevo elige «Vincular dispositivo» y pega aquí su código.';

  @override
  String get deviceCode => 'Código del dispositivo';

  @override
  String get scanNewDeviceQr => 'Escanea el QR del dispositivo nuevo';

  @override
  String get deviceName => 'Nombre del dispositivo';

  @override
  String get link => 'Vincular';

  @override
  String revokeTitle(String label) {
    return '¿Revocar «$label»?';
  }

  @override
  String get revokeBody =>
      'Dejará de recibir mensajes y se le expulsará de todos los chats.';

  @override
  String get revoke => 'Revocar';

  @override
  String get profile => 'Perfil';

  @override
  String get changePhoto => 'Cambiar foto';

  @override
  String get removePhoto => 'Quitar foto';

  @override
  String get photoVisibility =>
      'Tu foto solo la ven las personas con las que tienes un chat aceptado.';

  @override
  String get qrExplanation =>
      'Quien escanee o pegue este código podrá escribirte. Incluye tu clave pública y, si ya está publicado, tu servicio onion para encontrarte desde cualquier red.';

  @override
  String get qrCode => 'Código QR';

  @override
  String get refresh => 'Actualizar';

  @override
  String get network => 'Red';

  @override
  String get thisDevice => 'Este dispositivo';

  @override
  String get dataEncrypted => 'Datos cifrados en el disco';

  @override
  String get dataNotEncrypted => 'Datos sin cifrar en el disco';

  @override
  String get keyInKeystore =>
      'La clave la guarda el sistema (Keystore o llavero).';

  @override
  String get noKeystore => 'No hay llavero del sistema donde guardar la clave.';

  @override
  String get batteryFree => 'Recibe mensajes con la pantalla apagada';

  @override
  String get batteryRestricted =>
      'Con la pantalla apagada, Android puede cortar la conexión';

  @override
  String get batteryExplanation =>
      'Sin servidores no hay notificaciones push: la app sigue conectada en segundo plano.';

  @override
  String get allow => 'Permitir';

  @override
  String get devices => 'Dispositivos';

  @override
  String thisDeviceLabel(String label) {
    return '$label (este)';
  }

  @override
  String get blocked => 'Bloqueados';

  @override
  String get noBlocked => 'No has bloqueado a nadie';

  @override
  String get unblock => 'Desbloquear';

  @override
  String get cantShrinkImage => 'No se pudo reducir la imagen lo suficiente';

  @override
  String get emoji => 'Emoji';

  @override
  String get stickers => 'Stickers';

  @override
  String get noRecents => 'Sin recientes';

  @override
  String get searchEmoji => 'Buscar emoji';

  @override
  String get removeStickerTitle => '¿Quitar de tus stickers?';

  @override
  String get addStickers => 'Añadir stickers';

  @override
  String get sendSticker => 'Enviar sticker';

  @override
  String get notAnImage => 'no es una imagen';

  @override
  String stickerAnimatedTooBig(int kb, int side, int maxKb) {
    return 'es animada y ocupa $kb KB o mide ${side}px (máximo $maxKb KB y 1024 px)';
  }

  @override
  String stickerCantShrink(int maxKb) {
    return 'no se pudo reducir a $maxKb KB';
  }

  @override
  String deviceLinked(String handle) {
    return 'Dispositivo vinculado a $handle';
  }

  @override
  String chatRequestFrom(String name) {
    return 'Solicitud de chat de $name';
  }

  @override
  String newChatWith(String name) {
    return 'Nuevo chat: $name';
  }

  @override
  String get expelled => 'Este dispositivo ha sido expulsado de un chat';

  @override
  String unverifiedMember(String name) {
    return '⚠ Miembro no verificado en el chat: $name';
  }

  @override
  String get noticeFull => 'Quién escribe y el mensaje';

  @override
  String get noticeSender => 'Solo quién escribe';

  @override
  String get noticeNone => 'Solo «Tienes mensajes nuevos»';

  @override
  String get youHaveNewMessages => 'Tienes mensajes nuevos';

  @override
  String get chatRequestNotice => 'Solicitud de chat';

  @override
  String get newMessage => 'Mensaje nuevo';

  @override
  String get messageNotices => 'Avisos de mensajes';

  @override
  String get messageNoticesBody =>
      'Los avisos se ven en la pantalla, a veces en la de bloqueo, y quedan en el historial del sistema. ¿Qué quieres que digan?';

  @override
  String get linkInstructionsScan =>
      'En el dispositivo principal abre Perfil → Dispositivos → Vincular y escanea el QR que te muestra.';

  @override
  String get scanPrimaryQr => 'Escanear el QR del principal';

  @override
  String get showOwnQr =>
      'Si el principal es un móvil, que escanee este código:';

  @override
  String get haveInvite => 'Pegar una invitación del principal';

  @override
  String get invite => 'Invitación';

  @override
  String get linkDialogInvite =>
      'En el dispositivo nuevo elige «Vincular dispositivo» y escanea este QR. Vale 10 minutos y una sola vez.';

  @override
  String get linkDialogOrCode =>
      '¿El dispositivo nuevo te muestra un código? Pégalo aquí:';

  @override
  String get linkDialogOrQr => 'O que el dispositivo nuevo escanee este QR:';

  @override
  String get waitingNewDevice => 'Esperando al dispositivo nuevo…';

  @override
  String get linkConnecting =>
      'Conectando con el otro dispositivo… Si estáis en redes distintas puede tardar un par de minutos.';

  @override
  String get linkFailed => 'No se pudo vincular';

  @override
  String get linkBadCode =>
      'Ese código no sirve para vincular. Comprueba que lo has copiado entero.';

  @override
  String get linkUnreachable =>
      'No se pudo contactar con el otro dispositivo. Comprueba que los dos tienen la app abierta y conexión a Internet. Si estáis en redes distintas, la conexión va por Tor: espera a que el código diga «Accesible desde otras redes» y vuelve a intentarlo.';

  @override
  String linkRefused(String reason) {
    return 'El otro dispositivo no lo aceptó ($reason). Pide una invitación nueva y vuelve a intentarlo.';
  }

  @override
  String deviceJoined(String label) {
    return '«$label» se ha vinculado';
  }

  @override
  String get labelPhone => 'móvil';

  @override
  String get labelComputer => 'ordenador';

  @override
  String get reachableAnywhere => 'Accesible desde otras redes';

  @override
  String get reachableLocal =>
      'De momento solo desde esta red (preparando Tor…)';

  @override
  String get ok => 'Aceptar';

  @override
  String get alwaysConnected => 'Siempre conectado a Tor';

  @override
  String get alwaysConnectedOn =>
      'Los mensajes llegan al momento. Gasta más batería.';

  @override
  String get alwaysConnectedOff =>
      'Con la app cerrada, se conecta de vez en cuando a recoger los mensajes. Android puede retrasar las comprobaciones.';

  @override
  String get checkEvery => 'Comprobar mensajes cada';

  @override
  String minutes(int n) {
    return '$n min';
  }

  @override
  String hours(int n) {
    String _temp0 = intl.Intl.pluralLogic(
      n,
      locale: localeName,
      other: '$n horas',
      one: '1 hora',
    );
    return '$_temp0';
  }
}
