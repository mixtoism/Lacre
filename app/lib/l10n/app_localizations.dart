import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:intl/intl.dart' as intl;

import 'app_localizations_en.dart';
import 'app_localizations_es.dart';

// ignore_for_file: type=lint

/// Callers can lookup localized strings with an instance of AppLocalizations
/// returned by `AppLocalizations.of(context)`.
///
/// Applications need to include `AppLocalizations.delegate()` in their app's
/// `localizationDelegates` list, and the locales they support in the app's
/// `supportedLocales` list. For example:
///
/// ```dart
/// import 'l10n/app_localizations.dart';
///
/// return MaterialApp(
///   localizationsDelegates: AppLocalizations.localizationsDelegates,
///   supportedLocales: AppLocalizations.supportedLocales,
///   home: MyApplicationHome(),
/// );
/// ```
///
/// ## Update pubspec.yaml
///
/// Please make sure to update your pubspec.yaml to include the following
/// packages:
///
/// ```yaml
/// dependencies:
///   # Internationalization support.
///   flutter_localizations:
///     sdk: flutter
///   intl: any # Use the pinned version from flutter_localizations
///
///   # Rest of dependencies
/// ```
///
/// ## iOS Applications
///
/// iOS applications define key application metadata, including supported
/// locales, in an Info.plist file that is built into the application bundle.
/// To configure the locales supported by your app, you’ll need to edit this
/// file.
///
/// First, open your project’s ios/Runner.xcworkspace Xcode workspace file.
/// Then, in the Project Navigator, open the Info.plist file under the Runner
/// project’s Runner folder.
///
/// Next, select the Information Property List item, select Add Item from the
/// Editor menu, then select Localizations from the pop-up menu.
///
/// Select and expand the newly-created Localizations item then, for each
/// locale your application supports, add a new item and select the locale
/// you wish to add from the pop-up menu in the Value field. This list should
/// be consistent with the languages listed in the AppLocalizations.supportedLocales
/// property.
abstract class AppLocalizations {
  AppLocalizations(String locale)
    : localeName = intl.Intl.canonicalizedLocale(locale.toString());

  final String localeName;

  static AppLocalizations of(BuildContext context) {
    return Localizations.of<AppLocalizations>(context, AppLocalizations)!;
  }

  static const LocalizationsDelegate<AppLocalizations> delegate =
      _AppLocalizationsDelegate();

  /// A list of this localizations delegate along with the default localizations
  /// delegates.
  ///
  /// Returns a list of localizations delegates containing this delegate along with
  /// GlobalMaterialLocalizations.delegate, GlobalCupertinoLocalizations.delegate,
  /// and GlobalWidgetsLocalizations.delegate.
  ///
  /// Additional delegates can be added by appending to this list in
  /// MaterialApp. This list does not have to be used at all if a custom list
  /// of delegates is preferred or required.
  static const List<LocalizationsDelegate<dynamic>> localizationsDelegates =
      <LocalizationsDelegate<dynamic>>[
        delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
      ];

  /// A list of this localizations delegate's supported locales.
  static const List<Locale> supportedLocales = <Locale>[
    Locale('en'),
    Locale('es'),
  ];

  /// No description provided for @startError.
  ///
  /// In es, this message translates to:
  /// **'No se pudo arrancar el nodo:\n\n{error}'**
  String startError(String error);

  /// No description provided for @copied.
  ///
  /// In es, this message translates to:
  /// **'{what} copiado'**
  String copied(String what);

  /// No description provided for @copy.
  ///
  /// In es, this message translates to:
  /// **'Copiar'**
  String get copy;

  /// No description provided for @cancel.
  ///
  /// In es, this message translates to:
  /// **'Cancelar'**
  String get cancel;

  /// No description provided for @noAppToOpen.
  ///
  /// In es, this message translates to:
  /// **'Ninguna app puede abrir este archivo'**
  String get noAppToOpen;

  /// No description provided for @cantOpenFile.
  ///
  /// In es, this message translates to:
  /// **'No se pudo abrir el archivo'**
  String get cantOpenFile;

  /// No description provided for @onboardingTagline.
  ///
  /// In es, this message translates to:
  /// **'Mensajería cifrada de extremo a extremo, sin servidores. Tus claves no salen de tus dispositivos.'**
  String get onboardingTagline;

  /// No description provided for @newIdentity.
  ///
  /// In es, this message translates to:
  /// **'Identidad nueva'**
  String get newIdentity;

  /// No description provided for @linkDevice.
  ///
  /// In es, this message translates to:
  /// **'Vincular dispositivo'**
  String get linkDevice;

  /// No description provided for @name.
  ///
  /// In es, this message translates to:
  /// **'Nombre'**
  String get name;

  /// No description provided for @nameHelper.
  ///
  /// In es, this message translates to:
  /// **'Minúsculas, números y _. Se le añadirá un sufijo: nombre#xxxxxxxx'**
  String get nameHelper;

  /// No description provided for @createIdentity.
  ///
  /// In es, this message translates to:
  /// **'Crear identidad'**
  String get createIdentity;

  /// No description provided for @linkInstructions.
  ///
  /// In es, this message translates to:
  /// **'En el dispositivo principal abre Perfil → Dispositivos → Vincular y escanea este QR o pega el código. Esta pantalla cambiará sola cuando se complete.'**
  String get linkInstructions;

  /// No description provided for @code.
  ///
  /// In es, this message translates to:
  /// **'Código'**
  String get code;

  /// No description provided for @waitingPrimary.
  ///
  /// In es, this message translates to:
  /// **'Esperando al dispositivo principal…'**
  String get waitingPrimary;

  /// No description provided for @noCameraPermission.
  ///
  /// In es, this message translates to:
  /// **'Sin permiso para usar la cámara'**
  String get noCameraPermission;

  /// No description provided for @cantOpenCamera.
  ///
  /// In es, this message translates to:
  /// **'No se pudo abrir la cámara: {code}'**
  String cantOpenCamera(String code);

  /// No description provided for @chat.
  ///
  /// In es, this message translates to:
  /// **'Chat'**
  String get chat;

  /// No description provided for @closeSearch.
  ///
  /// In es, this message translates to:
  /// **'Cerrar búsqueda'**
  String get closeSearch;

  /// No description provided for @searchAllChats.
  ///
  /// In es, this message translates to:
  /// **'Buscar en todos los chats'**
  String get searchAllChats;

  /// No description provided for @clear.
  ///
  /// In es, this message translates to:
  /// **'Borrar'**
  String get clear;

  /// No description provided for @search.
  ///
  /// In es, this message translates to:
  /// **'Buscar'**
  String get search;

  /// No description provided for @newChat.
  ///
  /// In es, this message translates to:
  /// **'Nuevo chat'**
  String get newChat;

  /// No description provided for @noChatsYet.
  ///
  /// In es, this message translates to:
  /// **'Aún no tienes chats'**
  String get noChatsYet;

  /// No description provided for @noChatsHint.
  ///
  /// In es, this message translates to:
  /// **'Pulsa «Nuevo chat» y escribe el handle de alguien o escanea su QR.'**
  String get noChatsHint;

  /// No description provided for @chatRequests.
  ///
  /// In es, this message translates to:
  /// **'Solicitudes de chat'**
  String get chatRequests;

  /// No description provided for @chatRequestsHint.
  ///
  /// In es, this message translates to:
  /// **'De personas con las que aún no hablas. Hasta que aceptes no se muestran sus imágenes ni se descargan sus archivos.'**
  String get chatRequestsHint;

  /// No description provided for @chats.
  ///
  /// In es, this message translates to:
  /// **'Chats'**
  String get chats;

  /// No description provided for @sticker.
  ///
  /// In es, this message translates to:
  /// **'Sticker'**
  String get sticker;

  /// No description provided for @you.
  ///
  /// In es, this message translates to:
  /// **'Tú'**
  String get you;

  /// No description provided for @request.
  ///
  /// In es, this message translates to:
  /// **'Solicitud'**
  String get request;

  /// No description provided for @notInChat.
  ///
  /// In es, this message translates to:
  /// **'Ya no participas en este chat'**
  String get notInChat;

  /// No description provided for @groupNoMessages.
  ///
  /// In es, this message translates to:
  /// **'Grupo · sin mensajes'**
  String get groupNoMessages;

  /// No description provided for @noMessages.
  ///
  /// In es, this message translates to:
  /// **'Sin mensajes'**
  String get noMessages;

  /// No description provided for @noResults.
  ///
  /// In es, this message translates to:
  /// **'Sin resultados'**
  String get noResults;

  /// No description provided for @messageCount.
  ///
  /// In es, this message translates to:
  /// **'{count, plural, =1{1 mensaje} other{{count} mensajes}}'**
  String messageCount(int count);

  /// No description provided for @peersConnected.
  ///
  /// In es, this message translates to:
  /// **'{count} peers conectados'**
  String peersConnected(int count);

  /// No description provided for @onionPublished.
  ///
  /// In es, this message translates to:
  /// **'Servicio onion publicado'**
  String get onionPublished;

  /// No description provided for @onionPublishing.
  ///
  /// In es, this message translates to:
  /// **'Servicio onion: publicando…'**
  String get onionPublishing;

  /// No description provided for @reachableByRelay.
  ///
  /// In es, this message translates to:
  /// **'Alcanzable por relay'**
  String get reachableByRelay;

  /// No description provided for @offline.
  ///
  /// In es, this message translates to:
  /// **'sin red'**
  String get offline;

  /// No description provided for @scanProfileQr.
  ///
  /// In es, this message translates to:
  /// **'Escanea el QR de su perfil'**
  String get scanProfileQr;

  /// No description provided for @participants.
  ///
  /// In es, this message translates to:
  /// **'Participantes'**
  String get participants;

  /// No description provided for @participantsHelper.
  ///
  /// In es, this message translates to:
  /// **'Handles (nombre#sufijo) o códigos QR (p2m:…),\nseparados por espacios o líneas'**
  String get participantsHelper;

  /// No description provided for @scanQr.
  ///
  /// In es, this message translates to:
  /// **'Escanear QR'**
  String get scanQr;

  /// No description provided for @groupTitleOptional.
  ///
  /// In es, this message translates to:
  /// **'Título del grupo (opcional)'**
  String get groupTitleOptional;

  /// No description provided for @searchingNetwork.
  ///
  /// In es, this message translates to:
  /// **'Buscando en la red… (por Tor puede tardar un minuto)'**
  String get searchingNetwork;

  /// No description provided for @create.
  ///
  /// In es, this message translates to:
  /// **'Crear'**
  String get create;

  /// No description provided for @deleteChatBodyDirect.
  ///
  /// In es, this message translates to:
  /// **'Se borran sus mensajes de este dispositivo. Si alguien vuelve a escribir, el chat reaparecerá vacío.'**
  String get deleteChatBodyDirect;

  /// No description provided for @deleteChatBodyManager.
  ///
  /// In es, this message translates to:
  /// **'Se borran sus mensajes de este dispositivo. Como el grupo lo gestiona este dispositivo, sigues en él: los demás pueden seguir escribiendo y el chat reaparecerá vacío.'**
  String get deleteChatBodyManager;

  /// No description provided for @deleteChatBodyLeave.
  ///
  /// In es, this message translates to:
  /// **'Saldrás del grupo en todos tus dispositivos y se borrarán sus mensajes de este. Solo volverás si alguien te añade de nuevo.'**
  String get deleteChatBodyLeave;

  /// No description provided for @deleteChatTitle.
  ///
  /// In es, this message translates to:
  /// **'¿Borrar «{name}»?'**
  String deleteChatTitle(String name);

  /// No description provided for @delete.
  ///
  /// In es, this message translates to:
  /// **'Borrar'**
  String get delete;

  /// No description provided for @leaveAndDelete.
  ///
  /// In es, this message translates to:
  /// **'Salir y borrar'**
  String get leaveAndDelete;

  /// No description provided for @blockTitle.
  ///
  /// In es, this message translates to:
  /// **'¿Bloquear a {name}?'**
  String blockTitle(String name);

  /// No description provided for @blockBody.
  ///
  /// In es, this message translates to:
  /// **'No verás sus mensajes ni sus invitaciones a grupos, y no podrá ver tu foto de perfil. No se le avisa. Puedes desbloquearle desde tu perfil.'**
  String get blockBody;

  /// No description provided for @block.
  ///
  /// In es, this message translates to:
  /// **'Bloquear'**
  String get block;

  /// No description provided for @blockPerson.
  ///
  /// In es, this message translates to:
  /// **'Bloquear a {name}'**
  String blockPerson(String name);

  /// No description provided for @acceptRequest.
  ///
  /// In es, this message translates to:
  /// **'Aceptar solicitud'**
  String get acceptRequest;

  /// No description provided for @deleteChat.
  ///
  /// In es, this message translates to:
  /// **'Borrar chat'**
  String get deleteChat;

  /// No description provided for @removeTitle.
  ///
  /// In es, this message translates to:
  /// **'¿Quitar a {handle}?'**
  String removeTitle(String handle);

  /// No description provided for @removeBody.
  ///
  /// In es, this message translates to:
  /// **'Saldrán del chat todos sus dispositivos y no recibirá los mensajes nuevos.'**
  String get removeBody;

  /// No description provided for @safetyTitle.
  ///
  /// In es, this message translates to:
  /// **'Número de seguridad con {handle}'**
  String safetyTitle(String handle);

  /// No description provided for @safetyBody.
  ///
  /// In es, this message translates to:
  /// **'Comparadlo en persona o por otro canal (una llamada). Si coincide con el que ve en su dispositivo, nadie está suplantando a esta persona.'**
  String get safetyBody;

  /// No description provided for @safetyNumber.
  ///
  /// In es, this message translates to:
  /// **'número de seguridad'**
  String get safetyNumber;

  /// No description provided for @close.
  ///
  /// In es, this message translates to:
  /// **'Cerrar'**
  String get close;

  /// No description provided for @remove.
  ///
  /// In es, this message translates to:
  /// **'Quitar'**
  String get remove;

  /// No description provided for @members.
  ///
  /// In es, this message translates to:
  /// **'Miembros'**
  String get members;

  /// No description provided for @add.
  ///
  /// In es, this message translates to:
  /// **'Añadir'**
  String get add;

  /// No description provided for @onlyCreatorEdits.
  ///
  /// In es, this message translates to:
  /// **'Solo quien creó el chat puede añadir o quitar personas.'**
  String get onlyCreatorEdits;

  /// No description provided for @removePerson.
  ///
  /// In es, this message translates to:
  /// **'Quitar a {handle}'**
  String removePerson(String handle);

  /// No description provided for @addPeople.
  ///
  /// In es, this message translates to:
  /// **'Añadir personas'**
  String get addPeople;

  /// No description provided for @people.
  ///
  /// In es, this message translates to:
  /// **'Personas'**
  String get people;

  /// No description provided for @peopleHelper.
  ///
  /// In es, this message translates to:
  /// **'Handles (nombre#sufijo) o códigos QR (p2m:…)'**
  String get peopleHelper;

  /// No description provided for @fileTooBig.
  ///
  /// In es, this message translates to:
  /// **'El archivo ocupa {size} (máximo {max})'**
  String fileTooBig(String size, String max);

  /// No description provided for @deliveryFailed.
  ///
  /// In es, this message translates to:
  /// **'{count, plural, =1{No se pudo entregar a 1 dispositivo; se reintentará} other{No se pudo entregar a {count} dispositivos; se reintentará}}'**
  String deliveryFailed(int count);

  /// No description provided for @notInChatDot.
  ///
  /// In es, this message translates to:
  /// **'Ya no participas en este chat.'**
  String get notInChatDot;

  /// No description provided for @deviceCount.
  ///
  /// In es, this message translates to:
  /// **'{count, plural, =1{1 dispositivo} other{{count} dispositivos}}'**
  String deviceCount(int count);

  /// No description provided for @searchInChat.
  ///
  /// In es, this message translates to:
  /// **'Buscar en el chat'**
  String get searchInChat;

  /// No description provided for @more.
  ///
  /// In es, this message translates to:
  /// **'Más'**
  String get more;

  /// No description provided for @searchThisChat.
  ///
  /// In es, this message translates to:
  /// **'Buscar en este chat'**
  String get searchThisChat;

  /// No description provided for @previousOlder.
  ///
  /// In es, this message translates to:
  /// **'Anterior (más antiguo)'**
  String get previousOlder;

  /// No description provided for @nextNewer.
  ///
  /// In es, this message translates to:
  /// **'Siguiente (más reciente)'**
  String get nextNewer;

  /// No description provided for @emojiAndStickers.
  ///
  /// In es, this message translates to:
  /// **'Emoji y stickers'**
  String get emojiAndStickers;

  /// No description provided for @attachFile.
  ///
  /// In es, this message translates to:
  /// **'Adjuntar archivo'**
  String get attachFile;

  /// No description provided for @message.
  ///
  /// In es, this message translates to:
  /// **'Mensaje'**
  String get message;

  /// No description provided for @send.
  ///
  /// In es, this message translates to:
  /// **'Enviar'**
  String get send;

  /// No description provided for @wantsToChat.
  ///
  /// In es, this message translates to:
  /// **'{name} quiere chatear contigo'**
  String wantsToChat(String name);

  /// No description provided for @invitedTo.
  ///
  /// In es, this message translates to:
  /// **'Te han invitado a «{name}»'**
  String invitedTo(String name);

  /// No description provided for @requestBannerBody.
  ///
  /// In es, this message translates to:
  /// **'Aún no habláis. Hasta que aceptes no se muestran sus stickers ni se descargan sus archivos, y no ve tu foto de perfil.'**
  String get requestBannerBody;

  /// No description provided for @accept.
  ///
  /// In es, this message translates to:
  /// **'Aceptar'**
  String get accept;

  /// No description provided for @alreadyInStickers.
  ///
  /// In es, this message translates to:
  /// **'Ya está en tus stickers'**
  String get alreadyInStickers;

  /// No description provided for @saveToStickers.
  ///
  /// In es, this message translates to:
  /// **'Guardar en mis stickers'**
  String get saveToStickers;

  /// No description provided for @savedToStickers.
  ///
  /// In es, this message translates to:
  /// **'Guardado en tus stickers'**
  String get savedToStickers;

  /// No description provided for @stickerHidden.
  ///
  /// In es, this message translates to:
  /// **'Sticker oculto hasta que aceptes'**
  String get stickerHidden;

  /// No description provided for @open.
  ///
  /// In es, this message translates to:
  /// **'Abrir'**
  String get open;

  /// No description provided for @acceptToDownload.
  ///
  /// In es, this message translates to:
  /// **'Acepta la solicitud para descargarlo'**
  String get acceptToDownload;

  /// No description provided for @download.
  ///
  /// In es, this message translates to:
  /// **'Descargar'**
  String get download;

  /// No description provided for @images.
  ///
  /// In es, this message translates to:
  /// **'Imágenes'**
  String get images;

  /// No description provided for @defaultDeviceLabel.
  ///
  /// In es, this message translates to:
  /// **'portátil'**
  String get defaultDeviceLabel;

  /// No description provided for @linkDialogScan.
  ///
  /// In es, this message translates to:
  /// **'En el dispositivo nuevo elige «Vincular dispositivo» y escanea su QR o pega aquí su código.'**
  String get linkDialogScan;

  /// No description provided for @linkDialogPaste.
  ///
  /// In es, this message translates to:
  /// **'En el dispositivo nuevo elige «Vincular dispositivo» y pega aquí su código.'**
  String get linkDialogPaste;

  /// No description provided for @deviceCode.
  ///
  /// In es, this message translates to:
  /// **'Código del dispositivo'**
  String get deviceCode;

  /// No description provided for @scanNewDeviceQr.
  ///
  /// In es, this message translates to:
  /// **'Escanea el QR del dispositivo nuevo'**
  String get scanNewDeviceQr;

  /// No description provided for @deviceName.
  ///
  /// In es, this message translates to:
  /// **'Nombre del dispositivo'**
  String get deviceName;

  /// No description provided for @link.
  ///
  /// In es, this message translates to:
  /// **'Vincular'**
  String get link;

  /// No description provided for @revokeTitle.
  ///
  /// In es, this message translates to:
  /// **'¿Revocar «{label}»?'**
  String revokeTitle(String label);

  /// No description provided for @revokeBody.
  ///
  /// In es, this message translates to:
  /// **'Dejará de recibir mensajes y se le expulsará de todos los chats.'**
  String get revokeBody;

  /// No description provided for @revoke.
  ///
  /// In es, this message translates to:
  /// **'Revocar'**
  String get revoke;

  /// No description provided for @profile.
  ///
  /// In es, this message translates to:
  /// **'Perfil'**
  String get profile;

  /// No description provided for @changePhoto.
  ///
  /// In es, this message translates to:
  /// **'Cambiar foto'**
  String get changePhoto;

  /// No description provided for @removePhoto.
  ///
  /// In es, this message translates to:
  /// **'Quitar foto'**
  String get removePhoto;

  /// No description provided for @photoVisibility.
  ///
  /// In es, this message translates to:
  /// **'Tu foto solo la ven las personas con las que tienes un chat aceptado.'**
  String get photoVisibility;

  /// No description provided for @qrExplanation.
  ///
  /// In es, this message translates to:
  /// **'Quien escanee o pegue este código podrá escribirte. Incluye tu clave pública y, si ya está publicado, tu servicio onion para encontrarte desde cualquier red.'**
  String get qrExplanation;

  /// No description provided for @qrCode.
  ///
  /// In es, this message translates to:
  /// **'Código QR'**
  String get qrCode;

  /// No description provided for @refresh.
  ///
  /// In es, this message translates to:
  /// **'Actualizar'**
  String get refresh;

  /// No description provided for @network.
  ///
  /// In es, this message translates to:
  /// **'Red'**
  String get network;

  /// No description provided for @thisDevice.
  ///
  /// In es, this message translates to:
  /// **'Este dispositivo'**
  String get thisDevice;

  /// No description provided for @dataEncrypted.
  ///
  /// In es, this message translates to:
  /// **'Datos cifrados en el disco'**
  String get dataEncrypted;

  /// No description provided for @dataNotEncrypted.
  ///
  /// In es, this message translates to:
  /// **'Datos sin cifrar en el disco'**
  String get dataNotEncrypted;

  /// No description provided for @keyInKeystore.
  ///
  /// In es, this message translates to:
  /// **'La clave la guarda el sistema (Keystore o llavero).'**
  String get keyInKeystore;

  /// No description provided for @noKeystore.
  ///
  /// In es, this message translates to:
  /// **'No hay llavero del sistema donde guardar la clave.'**
  String get noKeystore;

  /// No description provided for @batteryFree.
  ///
  /// In es, this message translates to:
  /// **'Recibe mensajes con la pantalla apagada'**
  String get batteryFree;

  /// No description provided for @batteryRestricted.
  ///
  /// In es, this message translates to:
  /// **'Con la pantalla apagada, Android puede cortar la conexión'**
  String get batteryRestricted;

  /// No description provided for @batteryExplanation.
  ///
  /// In es, this message translates to:
  /// **'Sin servidores no hay notificaciones push: la app sigue conectada en segundo plano.'**
  String get batteryExplanation;

  /// No description provided for @allow.
  ///
  /// In es, this message translates to:
  /// **'Permitir'**
  String get allow;

  /// No description provided for @devices.
  ///
  /// In es, this message translates to:
  /// **'Dispositivos'**
  String get devices;

  /// No description provided for @thisDeviceLabel.
  ///
  /// In es, this message translates to:
  /// **'{label} (este)'**
  String thisDeviceLabel(String label);

  /// No description provided for @blocked.
  ///
  /// In es, this message translates to:
  /// **'Bloqueados'**
  String get blocked;

  /// No description provided for @noBlocked.
  ///
  /// In es, this message translates to:
  /// **'No has bloqueado a nadie'**
  String get noBlocked;

  /// No description provided for @unblock.
  ///
  /// In es, this message translates to:
  /// **'Desbloquear'**
  String get unblock;

  /// No description provided for @cantShrinkImage.
  ///
  /// In es, this message translates to:
  /// **'No se pudo reducir la imagen lo suficiente'**
  String get cantShrinkImage;

  /// No description provided for @emoji.
  ///
  /// In es, this message translates to:
  /// **'Emoji'**
  String get emoji;

  /// No description provided for @stickers.
  ///
  /// In es, this message translates to:
  /// **'Stickers'**
  String get stickers;

  /// No description provided for @noRecents.
  ///
  /// In es, this message translates to:
  /// **'Sin recientes'**
  String get noRecents;

  /// No description provided for @searchEmoji.
  ///
  /// In es, this message translates to:
  /// **'Buscar emoji'**
  String get searchEmoji;

  /// No description provided for @removeStickerTitle.
  ///
  /// In es, this message translates to:
  /// **'¿Quitar de tus stickers?'**
  String get removeStickerTitle;

  /// No description provided for @addStickers.
  ///
  /// In es, this message translates to:
  /// **'Añadir stickers'**
  String get addStickers;

  /// No description provided for @sendSticker.
  ///
  /// In es, this message translates to:
  /// **'Enviar sticker'**
  String get sendSticker;

  /// No description provided for @notAnImage.
  ///
  /// In es, this message translates to:
  /// **'no es una imagen'**
  String get notAnImage;

  /// No description provided for @stickerAnimatedTooBig.
  ///
  /// In es, this message translates to:
  /// **'es animada y ocupa {kb} KB o mide {side}px (máximo {maxKb} KB y 1024 px)'**
  String stickerAnimatedTooBig(int kb, int side, int maxKb);

  /// No description provided for @stickerCantShrink.
  ///
  /// In es, this message translates to:
  /// **'no se pudo reducir a {maxKb} KB'**
  String stickerCantShrink(int maxKb);

  /// No description provided for @deviceLinked.
  ///
  /// In es, this message translates to:
  /// **'Dispositivo vinculado a {handle}'**
  String deviceLinked(String handle);

  /// No description provided for @chatRequestFrom.
  ///
  /// In es, this message translates to:
  /// **'Solicitud de chat de {name}'**
  String chatRequestFrom(String name);

  /// No description provided for @newChatWith.
  ///
  /// In es, this message translates to:
  /// **'Nuevo chat: {name}'**
  String newChatWith(String name);

  /// No description provided for @expelled.
  ///
  /// In es, this message translates to:
  /// **'Este dispositivo ha sido expulsado de un chat'**
  String get expelled;

  /// No description provided for @unverifiedMember.
  ///
  /// In es, this message translates to:
  /// **'⚠ Miembro no verificado en el chat: {name}'**
  String unverifiedMember(String name);

  /// No description provided for @noticeFull.
  ///
  /// In es, this message translates to:
  /// **'Quién escribe y el mensaje'**
  String get noticeFull;

  /// No description provided for @noticeSender.
  ///
  /// In es, this message translates to:
  /// **'Solo quién escribe'**
  String get noticeSender;

  /// No description provided for @noticeNone.
  ///
  /// In es, this message translates to:
  /// **'Solo «Tienes mensajes nuevos»'**
  String get noticeNone;

  /// No description provided for @youHaveNewMessages.
  ///
  /// In es, this message translates to:
  /// **'Tienes mensajes nuevos'**
  String get youHaveNewMessages;

  /// No description provided for @chatRequestNotice.
  ///
  /// In es, this message translates to:
  /// **'Solicitud de chat'**
  String get chatRequestNotice;

  /// No description provided for @newMessage.
  ///
  /// In es, this message translates to:
  /// **'Mensaje nuevo'**
  String get newMessage;

  /// No description provided for @messageNotices.
  ///
  /// In es, this message translates to:
  /// **'Avisos de mensajes'**
  String get messageNotices;

  /// No description provided for @messageNoticesBody.
  ///
  /// In es, this message translates to:
  /// **'Los avisos se ven en la pantalla, a veces en la de bloqueo, y quedan en el historial del sistema. ¿Qué quieres que digan?'**
  String get messageNoticesBody;

  /// No description provided for @linkInstructionsScan.
  ///
  /// In es, this message translates to:
  /// **'En el dispositivo principal abre Perfil → Dispositivos → Vincular y escanea el QR que te muestra.'**
  String get linkInstructionsScan;

  /// No description provided for @scanPrimaryQr.
  ///
  /// In es, this message translates to:
  /// **'Escanear el QR del principal'**
  String get scanPrimaryQr;

  /// No description provided for @showOwnQr.
  ///
  /// In es, this message translates to:
  /// **'Si el principal es un móvil, que escanee este código:'**
  String get showOwnQr;

  /// No description provided for @haveInvite.
  ///
  /// In es, this message translates to:
  /// **'Pegar una invitación del principal'**
  String get haveInvite;

  /// No description provided for @invite.
  ///
  /// In es, this message translates to:
  /// **'Invitación'**
  String get invite;

  /// No description provided for @linkDialogInvite.
  ///
  /// In es, this message translates to:
  /// **'En el dispositivo nuevo elige «Vincular dispositivo» y escanea este QR. Vale 10 minutos y una sola vez.'**
  String get linkDialogInvite;

  /// No description provided for @linkDialogOrCode.
  ///
  /// In es, this message translates to:
  /// **'¿El dispositivo nuevo te muestra un código? Pégalo aquí:'**
  String get linkDialogOrCode;

  /// No description provided for @linkDialogOrQr.
  ///
  /// In es, this message translates to:
  /// **'O que el dispositivo nuevo escanee este QR:'**
  String get linkDialogOrQr;

  /// No description provided for @waitingNewDevice.
  ///
  /// In es, this message translates to:
  /// **'Esperando al dispositivo nuevo…'**
  String get waitingNewDevice;

  /// No description provided for @linkConnecting.
  ///
  /// In es, this message translates to:
  /// **'Conectando con el otro dispositivo… Si estáis en redes distintas puede tardar un par de minutos.'**
  String get linkConnecting;

  /// No description provided for @linkFailed.
  ///
  /// In es, this message translates to:
  /// **'No se pudo vincular'**
  String get linkFailed;

  /// No description provided for @linkBadCode.
  ///
  /// In es, this message translates to:
  /// **'Ese código no sirve para vincular. Comprueba que lo has copiado entero.'**
  String get linkBadCode;

  /// No description provided for @linkUnreachable.
  ///
  /// In es, this message translates to:
  /// **'No se pudo contactar con el otro dispositivo. Comprueba que los dos tienen la app abierta y conexión a Internet. Si estáis en redes distintas, la conexión va por Tor: espera a que el código diga «Accesible desde otras redes» y vuelve a intentarlo.'**
  String get linkUnreachable;

  /// No description provided for @linkRefused.
  ///
  /// In es, this message translates to:
  /// **'El otro dispositivo no lo aceptó ({reason}). Pide una invitación nueva y vuelve a intentarlo.'**
  String linkRefused(String reason);

  /// No description provided for @deviceJoined.
  ///
  /// In es, this message translates to:
  /// **'«{label}» se ha vinculado'**
  String deviceJoined(String label);

  /// No description provided for @labelPhone.
  ///
  /// In es, this message translates to:
  /// **'móvil'**
  String get labelPhone;

  /// No description provided for @labelComputer.
  ///
  /// In es, this message translates to:
  /// **'ordenador'**
  String get labelComputer;

  /// No description provided for @reachableAnywhere.
  ///
  /// In es, this message translates to:
  /// **'Accesible desde otras redes'**
  String get reachableAnywhere;

  /// No description provided for @reachableLocal.
  ///
  /// In es, this message translates to:
  /// **'De momento solo desde esta red (preparando Tor…)'**
  String get reachableLocal;

  /// No description provided for @ok.
  ///
  /// In es, this message translates to:
  /// **'Aceptar'**
  String get ok;

  /// No description provided for @alwaysConnected.
  ///
  /// In es, this message translates to:
  /// **'Siempre conectado a Tor'**
  String get alwaysConnected;

  /// No description provided for @alwaysConnectedOn.
  ///
  /// In es, this message translates to:
  /// **'Los mensajes llegan al momento. Gasta más batería.'**
  String get alwaysConnectedOn;

  /// No description provided for @alwaysConnectedOff.
  ///
  /// In es, this message translates to:
  /// **'Con la app cerrada, se conecta de vez en cuando a recoger los mensajes. Android puede retrasar las comprobaciones.'**
  String get alwaysConnectedOff;

  /// No description provided for @checkEvery.
  ///
  /// In es, this message translates to:
  /// **'Comprobar mensajes cada'**
  String get checkEvery;

  /// No description provided for @minutes.
  ///
  /// In es, this message translates to:
  /// **'{n} min'**
  String minutes(int n);

  /// No description provided for @hours.
  ///
  /// In es, this message translates to:
  /// **'{n, plural, =1{1 hora} other{{n} horas}}'**
  String hours(int n);
}

class _AppLocalizationsDelegate
    extends LocalizationsDelegate<AppLocalizations> {
  const _AppLocalizationsDelegate();

  @override
  Future<AppLocalizations> load(Locale locale) {
    return SynchronousFuture<AppLocalizations>(lookupAppLocalizations(locale));
  }

  @override
  bool isSupported(Locale locale) =>
      <String>['en', 'es'].contains(locale.languageCode);

  @override
  bool shouldReload(_AppLocalizationsDelegate old) => false;
}

AppLocalizations lookupAppLocalizations(Locale locale) {
  // Lookup logic when only language code is specified.
  switch (locale.languageCode) {
    case 'en':
      return AppLocalizationsEn();
    case 'es':
      return AppLocalizationsEs();
  }

  throw FlutterError(
    'AppLocalizations.delegate failed to load unsupported locale "$locale". This is likely '
    'an issue with the localizations generation tool. Please file an issue '
    'on GitHub with a reproducible sample app and the gen-l10n configuration '
    'that was used.',
  );
}
