// ignore: unused_import
import 'package:intl/intl.dart' as intl;

import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for English (`en`).
class AppLocalizationsEn extends AppLocalizations {
  AppLocalizationsEn([String locale = 'en']) : super(locale);

  @override
  String startError(String error) {
    return 'Couldn\'t start the node:\n\n$error';
  }

  @override
  String copied(String what) {
    return '$what copied';
  }

  @override
  String get copy => 'Copy';

  @override
  String get cancel => 'Cancel';

  @override
  String get noAppToOpen => 'No app can open this file';

  @override
  String get cantOpenFile => 'Couldn\'t open the file';

  @override
  String get onboardingTagline =>
      'End-to-end encrypted messaging, with no servers. Your keys never leave your devices.';

  @override
  String get newIdentity => 'New identity';

  @override
  String get linkDevice => 'Link device';

  @override
  String get name => 'Name';

  @override
  String get nameHelper =>
      'Lowercase letters, numbers and _. A suffix will be added: name#xxxxxxxx';

  @override
  String get createIdentity => 'Create identity';

  @override
  String get linkInstructions =>
      'On your main device open Profile → Devices → Link and scan this QR or paste the code. This screen will change by itself once it\'s done.';

  @override
  String get code => 'Code';

  @override
  String get waitingPrimary => 'Waiting for the main device…';

  @override
  String get noCameraPermission => 'No permission to use the camera';

  @override
  String cantOpenCamera(String code) {
    return 'Couldn\'t open the camera: $code';
  }

  @override
  String get chat => 'Chat';

  @override
  String get closeSearch => 'Close search';

  @override
  String get searchAllChats => 'Search all chats';

  @override
  String get clear => 'Clear';

  @override
  String get search => 'Search';

  @override
  String get newChat => 'New chat';

  @override
  String get noChatsYet => 'No chats yet';

  @override
  String get noChatsHint =>
      'Tap “New chat” and type someone\'s handle or scan their QR.';

  @override
  String get chatRequests => 'Chat requests';

  @override
  String get chatRequestsHint =>
      'From people you don\'t talk to yet. Until you accept, their images aren\'t shown and their files aren\'t downloaded.';

  @override
  String get chats => 'Chats';

  @override
  String get sticker => 'Sticker';

  @override
  String get you => 'You';

  @override
  String get request => 'Request';

  @override
  String get notInChat => 'You\'re no longer in this chat';

  @override
  String get groupNoMessages => 'Group · no messages';

  @override
  String get noMessages => 'No messages';

  @override
  String get noResults => 'No results';

  @override
  String messageCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count messages',
      one: '1 message',
    );
    return '$_temp0';
  }

  @override
  String peersConnected(int count) {
    return '$count peers connected';
  }

  @override
  String get onionPublished => 'Onion service published';

  @override
  String get onionPublishing => 'Onion service: publishing…';

  @override
  String get reachableByRelay => 'Reachable via relay';

  @override
  String get offline => 'offline';

  @override
  String get scanProfileQr => 'Scan their profile QR';

  @override
  String get participants => 'Participants';

  @override
  String get participantsHelper =>
      'Handles (name#suffix) or QR codes (p2m:…),\nseparated by spaces or lines';

  @override
  String get scanQr => 'Scan QR';

  @override
  String get groupTitleOptional => 'Group title (optional)';

  @override
  String get searchingNetwork =>
      'Searching the network… (over Tor it may take a minute)';

  @override
  String get create => 'Create';

  @override
  String get deleteChatBodyDirect =>
      'Its messages are deleted from this device. If someone writes again, the chat will reappear empty.';

  @override
  String get deleteChatBodyManager =>
      'Its messages are deleted from this device. Since this device manages the group, you stay in it: the others can keep writing and the chat will reappear empty.';

  @override
  String get deleteChatBodyLeave =>
      'You\'ll leave the group on all your devices and its messages will be deleted from this one. You\'ll only be back if someone adds you again.';

  @override
  String deleteChatTitle(String name) {
    return 'Delete “$name”?';
  }

  @override
  String get delete => 'Delete';

  @override
  String get leaveAndDelete => 'Leave and delete';

  @override
  String blockTitle(String name) {
    return 'Block $name?';
  }

  @override
  String get blockBody =>
      'You won\'t see their messages or group invitations, and they won\'t be able to see your profile photo. They aren\'t notified. You can unblock them from your profile.';

  @override
  String get block => 'Block';

  @override
  String blockPerson(String name) {
    return 'Block $name';
  }

  @override
  String get acceptRequest => 'Accept request';

  @override
  String get deleteChat => 'Delete chat';

  @override
  String removeTitle(String handle) {
    return 'Remove $handle?';
  }

  @override
  String get removeBody =>
      'All their devices will leave the chat and they won\'t receive new messages.';

  @override
  String safetyTitle(String handle) {
    return 'Safety number with $handle';
  }

  @override
  String get safetyBody =>
      'Compare it in person or over another channel (a call). If it matches the one on their device, nobody is impersonating this person.';

  @override
  String get safetyNumber => 'safety number';

  @override
  String get close => 'Close';

  @override
  String get remove => 'Remove';

  @override
  String get members => 'Members';

  @override
  String get add => 'Add';

  @override
  String get onlyCreatorEdits =>
      'Only the person who created the chat can add or remove people.';

  @override
  String removePerson(String handle) {
    return 'Remove $handle';
  }

  @override
  String get addPeople => 'Add people';

  @override
  String get people => 'People';

  @override
  String get peopleHelper => 'Handles (name#suffix) or QR codes (p2m:…)';

  @override
  String fileTooBig(String size, String max) {
    return 'The file is $size (maximum $max)';
  }

  @override
  String deliveryFailed(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: 'Couldn\'t deliver to $count devices; will retry',
      one: 'Couldn\'t deliver to 1 device; will retry',
    );
    return '$_temp0';
  }

  @override
  String get notInChatDot => 'You\'re no longer in this chat.';

  @override
  String deviceCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count devices',
      one: '1 device',
    );
    return '$_temp0';
  }

  @override
  String get searchInChat => 'Search in chat';

  @override
  String get more => 'More';

  @override
  String get searchThisChat => 'Search this chat';

  @override
  String get previousOlder => 'Previous (older)';

  @override
  String get nextNewer => 'Next (newer)';

  @override
  String get emojiAndStickers => 'Emoji and stickers';

  @override
  String get attachFile => 'Attach file';

  @override
  String get message => 'Message';

  @override
  String get send => 'Send';

  @override
  String wantsToChat(String name) {
    return '$name wants to chat with you';
  }

  @override
  String invitedTo(String name) {
    return 'You\'ve been invited to “$name”';
  }

  @override
  String get requestBannerBody =>
      'You haven\'t talked yet. Until you accept, their stickers aren\'t shown, their files aren\'t downloaded, and they can\'t see your profile photo.';

  @override
  String get accept => 'Accept';

  @override
  String get alreadyInStickers => 'Already in your stickers';

  @override
  String get saveToStickers => 'Save to my stickers';

  @override
  String get savedToStickers => 'Saved to your stickers';

  @override
  String get stickerHidden => 'Sticker hidden until you accept';

  @override
  String get open => 'Open';

  @override
  String get acceptToDownload => 'Accept the request to download it';

  @override
  String get download => 'Download';

  @override
  String get images => 'Images';

  @override
  String get defaultDeviceLabel => 'laptop';

  @override
  String get linkDialogScan =>
      'On the new device choose “Link device” and scan its QR or paste its code here.';

  @override
  String get linkDialogPaste =>
      'On the new device choose “Link device” and paste its code here.';

  @override
  String get deviceCode => 'Device code';

  @override
  String get scanNewDeviceQr => 'Scan the new device\'s QR';

  @override
  String get deviceName => 'Device name';

  @override
  String get link => 'Link';

  @override
  String revokeTitle(String label) {
    return 'Revoke “$label”?';
  }

  @override
  String get revokeBody =>
      'It will stop receiving messages and will be removed from all chats.';

  @override
  String get revoke => 'Revoke';

  @override
  String get profile => 'Profile';

  @override
  String get changePhoto => 'Change photo';

  @override
  String get removePhoto => 'Remove photo';

  @override
  String get photoVisibility =>
      'Your photo is only visible to people you have an accepted chat with.';

  @override
  String get qrExplanation =>
      'Anyone who scans or pastes this code can write to you. It includes your public key and, if already published, your onion service so you can be found from any network.';

  @override
  String get qrCode => 'QR code';

  @override
  String get refresh => 'Refresh';

  @override
  String get network => 'Network';

  @override
  String get thisDevice => 'This device';

  @override
  String get dataEncrypted => 'Data encrypted on disk';

  @override
  String get dataNotEncrypted => 'Data not encrypted on disk';

  @override
  String get keyInKeystore =>
      'The key is kept by the system (Keystore or keyring).';

  @override
  String get noKeystore => 'There\'s no system keyring to store the key in.';

  @override
  String get batteryFree => 'Receives messages with the screen off';

  @override
  String get batteryRestricted =>
      'With the screen off, Android may cut the connection';

  @override
  String get batteryExplanation =>
      'Without servers there are no push notifications: the app stays connected in the background.';

  @override
  String get allow => 'Allow';

  @override
  String get devices => 'Devices';

  @override
  String thisDeviceLabel(String label) {
    return '$label (this one)';
  }

  @override
  String get blocked => 'Blocked';

  @override
  String get noBlocked => 'You haven\'t blocked anyone';

  @override
  String get unblock => 'Unblock';

  @override
  String get cantShrinkImage => 'Couldn\'t shrink the image enough';

  @override
  String get emoji => 'Emoji';

  @override
  String get stickers => 'Stickers';

  @override
  String get noRecents => 'No recents';

  @override
  String get searchEmoji => 'Search emoji';

  @override
  String get removeStickerTitle => 'Remove from your stickers?';

  @override
  String get addStickers => 'Add stickers';

  @override
  String get sendSticker => 'Send sticker';

  @override
  String get notAnImage => 'not an image';

  @override
  String stickerAnimatedTooBig(int kb, int side, int maxKb) {
    return 'it\'s animated and is $kb KB or ${side}px (maximum $maxKb KB and 1024 px)';
  }

  @override
  String stickerCantShrink(int maxKb) {
    return 'couldn\'t shrink it to $maxKb KB';
  }

  @override
  String deviceLinked(String handle) {
    return 'Device linked to $handle';
  }

  @override
  String chatRequestFrom(String name) {
    return 'Chat request from $name';
  }

  @override
  String newChatWith(String name) {
    return 'New chat: $name';
  }

  @override
  String get expelled => 'This device has been removed from a chat';

  @override
  String unverifiedMember(String name) {
    return '⚠ Unverified member in the chat: $name';
  }

  @override
  String get noticeFull => 'Sender and message';

  @override
  String get noticeSender => 'Sender only';

  @override
  String get noticeNone => 'Only “You have new messages”';

  @override
  String get youHaveNewMessages => 'You have new messages';

  @override
  String get chatRequestNotice => 'Chat request';

  @override
  String get newMessage => 'New message';

  @override
  String get messageNotices => 'Message notifications';

  @override
  String get messageNoticesBody =>
      'Notifications show up on the screen, sometimes on the lock screen, and stay in the system history. What should they say?';

  @override
  String get linkInstructionsScan =>
      'On your main device open Profile → Devices → Link and scan the QR it shows you.';

  @override
  String get scanPrimaryQr => 'Scan the main device\'s QR';

  @override
  String get showOwnQr =>
      'If the main device is a phone, let it scan this code:';

  @override
  String get haveInvite => 'Paste an invitation from the main device';

  @override
  String get invite => 'Invitation';

  @override
  String get linkDialogInvite =>
      'On the new device choose “Link device” and scan this QR. It is valid for 10 minutes and only once.';

  @override
  String get linkDialogOrCode =>
      'Does the new device show you a code? Paste it here:';

  @override
  String get linkDialogOrQr => 'Or let the new device scan this QR:';

  @override
  String get waitingNewDevice => 'Waiting for the new device…';

  @override
  String get linkConnecting =>
      'Connecting to the other device… If you are on different networks it may take a couple of minutes.';

  @override
  String get linkFailed => 'Couldn\'t link';

  @override
  String get linkBadCode =>
      'That code can\'t be used to link. Check that you copied all of it.';

  @override
  String get linkUnreachable =>
      'Couldn\'t reach the other device. Check that both have the app open and an Internet connection. If you are on different networks the connection goes through Tor: wait until the code says “Reachable from other networks” and try again.';

  @override
  String linkRefused(String reason) {
    return 'The other device didn\'t accept it ($reason). Get a new invitation and try again.';
  }

  @override
  String deviceJoined(String label) {
    return '“$label” has been linked';
  }

  @override
  String get labelPhone => 'phone';

  @override
  String get labelComputer => 'computer';

  @override
  String get reachableAnywhere => 'Reachable from other networks';

  @override
  String get reachableLocal =>
      'Only from this network for now (setting up Tor…)';

  @override
  String get ok => 'OK';

  @override
  String get alwaysConnected => 'Always connected to Tor';

  @override
  String get alwaysConnectedOn =>
      'Messages arrive instantly. Uses more battery.';

  @override
  String get alwaysConnectedOff =>
      'With the app closed, it connects now and then to pick up messages. Android may delay the checks.';

  @override
  String get checkEvery => 'Check for messages every';

  @override
  String minutes(int n) {
    return '$n min';
  }

  @override
  String hours(int n) {
    String _temp0 = intl.Intl.pluralLogic(
      n,
      locale: localeName,
      other: '$n hours',
      one: '1 hour',
    );
    return '$_temp0';
  }
}
