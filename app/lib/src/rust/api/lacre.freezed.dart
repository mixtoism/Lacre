// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint, type=warning, deprecated_member_use, deprecated_member_use_from_same_package
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'lacre.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$UiEvent {





@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is UiEvent);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'UiEvent()';
}


}

/// @nodoc
class $UiEventCopyWith<$Res>  {
$UiEventCopyWith(UiEvent _, $Res Function(UiEvent) __);
}


/// Adds pattern-matching-related methods to [UiEvent].
extension UiEventPatterns on UiEvent {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( UiEvent_Linked value)?  linked,TResult Function( UiEvent_NewChat value)?  newChat,TResult Function( UiEvent_Avatar value)?  avatar,TResult Function( UiEvent_ChatsChanged value)?  chatsChanged,TResult Function( UiEvent_Message value)?  message,TResult Function( UiEvent_Removed value)?  removed,TResult Function( UiEvent_UnverifiedMember value)?  unverifiedMember,TResult Function( UiEvent_RelayReady value)?  relayReady,TResult Function( UiEvent_FileProgress value)?  fileProgress,required TResult orElse(),}){
final _that = this;
switch (_that) {
case UiEvent_Linked() when linked != null:
return linked(_that);case UiEvent_NewChat() when newChat != null:
return newChat(_that);case UiEvent_Avatar() when avatar != null:
return avatar(_that);case UiEvent_ChatsChanged() when chatsChanged != null:
return chatsChanged(_that);case UiEvent_Message() when message != null:
return message(_that);case UiEvent_Removed() when removed != null:
return removed(_that);case UiEvent_UnverifiedMember() when unverifiedMember != null:
return unverifiedMember(_that);case UiEvent_RelayReady() when relayReady != null:
return relayReady(_that);case UiEvent_FileProgress() when fileProgress != null:
return fileProgress(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( UiEvent_Linked value)  linked,required TResult Function( UiEvent_NewChat value)  newChat,required TResult Function( UiEvent_Avatar value)  avatar,required TResult Function( UiEvent_ChatsChanged value)  chatsChanged,required TResult Function( UiEvent_Message value)  message,required TResult Function( UiEvent_Removed value)  removed,required TResult Function( UiEvent_UnverifiedMember value)  unverifiedMember,required TResult Function( UiEvent_RelayReady value)  relayReady,required TResult Function( UiEvent_FileProgress value)  fileProgress,}){
final _that = this;
switch (_that) {
case UiEvent_Linked():
return linked(_that);case UiEvent_NewChat():
return newChat(_that);case UiEvent_Avatar():
return avatar(_that);case UiEvent_ChatsChanged():
return chatsChanged(_that);case UiEvent_Message():
return message(_that);case UiEvent_Removed():
return removed(_that);case UiEvent_UnverifiedMember():
return unverifiedMember(_that);case UiEvent_RelayReady():
return relayReady(_that);case UiEvent_FileProgress():
return fileProgress(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( UiEvent_Linked value)?  linked,TResult? Function( UiEvent_NewChat value)?  newChat,TResult? Function( UiEvent_Avatar value)?  avatar,TResult? Function( UiEvent_ChatsChanged value)?  chatsChanged,TResult? Function( UiEvent_Message value)?  message,TResult? Function( UiEvent_Removed value)?  removed,TResult? Function( UiEvent_UnverifiedMember value)?  unverifiedMember,TResult? Function( UiEvent_RelayReady value)?  relayReady,TResult? Function( UiEvent_FileProgress value)?  fileProgress,}){
final _that = this;
switch (_that) {
case UiEvent_Linked() when linked != null:
return linked(_that);case UiEvent_NewChat() when newChat != null:
return newChat(_that);case UiEvent_Avatar() when avatar != null:
return avatar(_that);case UiEvent_ChatsChanged() when chatsChanged != null:
return chatsChanged(_that);case UiEvent_Message() when message != null:
return message(_that);case UiEvent_Removed() when removed != null:
return removed(_that);case UiEvent_UnverifiedMember() when unverifiedMember != null:
return unverifiedMember(_that);case UiEvent_RelayReady() when relayReady != null:
return relayReady(_that);case UiEvent_FileProgress() when fileProgress != null:
return fileProgress(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( String handle)?  linked,TResult Function( int index,  String name,  bool request)?  newChat,TResult Function( String handle)?  avatar,TResult Function()?  chatsChanged,TResult Function( int index,  Message message,  bool local)?  message,TResult Function( int index)?  removed,TResult Function( int index,  String name)?  unverifiedMember,TResult Function( String addr)?  relayReady,TResult Function( String id,  int done,  int total)?  fileProgress,required TResult orElse(),}) {final _that = this;
switch (_that) {
case UiEvent_Linked() when linked != null:
return linked(_that.handle);case UiEvent_NewChat() when newChat != null:
return newChat(_that.index,_that.name,_that.request);case UiEvent_Avatar() when avatar != null:
return avatar(_that.handle);case UiEvent_ChatsChanged() when chatsChanged != null:
return chatsChanged();case UiEvent_Message() when message != null:
return message(_that.index,_that.message,_that.local);case UiEvent_Removed() when removed != null:
return removed(_that.index);case UiEvent_UnverifiedMember() when unverifiedMember != null:
return unverifiedMember(_that.index,_that.name);case UiEvent_RelayReady() when relayReady != null:
return relayReady(_that.addr);case UiEvent_FileProgress() when fileProgress != null:
return fileProgress(_that.id,_that.done,_that.total);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( String handle)  linked,required TResult Function( int index,  String name,  bool request)  newChat,required TResult Function( String handle)  avatar,required TResult Function()  chatsChanged,required TResult Function( int index,  Message message,  bool local)  message,required TResult Function( int index)  removed,required TResult Function( int index,  String name)  unverifiedMember,required TResult Function( String addr)  relayReady,required TResult Function( String id,  int done,  int total)  fileProgress,}) {final _that = this;
switch (_that) {
case UiEvent_Linked():
return linked(_that.handle);case UiEvent_NewChat():
return newChat(_that.index,_that.name,_that.request);case UiEvent_Avatar():
return avatar(_that.handle);case UiEvent_ChatsChanged():
return chatsChanged();case UiEvent_Message():
return message(_that.index,_that.message,_that.local);case UiEvent_Removed():
return removed(_that.index);case UiEvent_UnverifiedMember():
return unverifiedMember(_that.index,_that.name);case UiEvent_RelayReady():
return relayReady(_that.addr);case UiEvent_FileProgress():
return fileProgress(_that.id,_that.done,_that.total);}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( String handle)?  linked,TResult? Function( int index,  String name,  bool request)?  newChat,TResult? Function( String handle)?  avatar,TResult? Function()?  chatsChanged,TResult? Function( int index,  Message message,  bool local)?  message,TResult? Function( int index)?  removed,TResult? Function( int index,  String name)?  unverifiedMember,TResult? Function( String addr)?  relayReady,TResult? Function( String id,  int done,  int total)?  fileProgress,}) {final _that = this;
switch (_that) {
case UiEvent_Linked() when linked != null:
return linked(_that.handle);case UiEvent_NewChat() when newChat != null:
return newChat(_that.index,_that.name,_that.request);case UiEvent_Avatar() when avatar != null:
return avatar(_that.handle);case UiEvent_ChatsChanged() when chatsChanged != null:
return chatsChanged();case UiEvent_Message() when message != null:
return message(_that.index,_that.message,_that.local);case UiEvent_Removed() when removed != null:
return removed(_that.index);case UiEvent_UnverifiedMember() when unverifiedMember != null:
return unverifiedMember(_that.index,_that.name);case UiEvent_RelayReady() when relayReady != null:
return relayReady(_that.addr);case UiEvent_FileProgress() when fileProgress != null:
return fileProgress(_that.id,_that.done,_that.total);case _:
  return null;

}
}

}

/// @nodoc


class UiEvent_Linked extends UiEvent {
  const UiEvent_Linked({required this.handle}): super._();
  

 final  String handle;

/// Create a copy of UiEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiEvent_LinkedCopyWith<UiEvent_Linked> get copyWith => _$UiEvent_LinkedCopyWithImpl<UiEvent_Linked>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is UiEvent_Linked&&(identical(other.handle, handle) || other.handle == handle));
}


@override
int get hashCode {
    return Object.hash(runtimeType,handle);
}

@override
String toString() {
    return 'UiEvent.linked(handle: $handle)';
}


}

/// @nodoc
abstract mixin class $UiEvent_LinkedCopyWith<$Res> implements $UiEventCopyWith<$Res> {
  factory $UiEvent_LinkedCopyWith(UiEvent_Linked value, $Res Function(UiEvent_Linked) _then) = _$UiEvent_LinkedCopyWithImpl;
@useResult
$Res call({
 String handle
});




}
/// @nodoc
class _$UiEvent_LinkedCopyWithImpl<$Res>
    implements $UiEvent_LinkedCopyWith<$Res> {
  _$UiEvent_LinkedCopyWithImpl(this._self, this._then);

  final UiEvent_Linked _self;
  final $Res Function(UiEvent_Linked) _then;

/// Create a copy of UiEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? handle = null,}) {
  return _then(UiEvent_Linked(
handle: null == handle ? _self.handle : handle // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class UiEvent_NewChat extends UiEvent {
  const UiEvent_NewChat({required this.index, required this.name, required this.request}): super._();
  

 final  int index;
 final  String name;
 final  bool request;

/// Create a copy of UiEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiEvent_NewChatCopyWith<UiEvent_NewChat> get copyWith => _$UiEvent_NewChatCopyWithImpl<UiEvent_NewChat>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is UiEvent_NewChat&&(identical(other.index, index) || other.index == index)&&(identical(other.name, name) || other.name == name)&&(identical(other.request, request) || other.request == request));
}


@override
int get hashCode {
    return Object.hash(runtimeType,index,name,request);
}

@override
String toString() {
    return 'UiEvent.newChat(index: $index, name: $name, request: $request)';
}


}

/// @nodoc
abstract mixin class $UiEvent_NewChatCopyWith<$Res> implements $UiEventCopyWith<$Res> {
  factory $UiEvent_NewChatCopyWith(UiEvent_NewChat value, $Res Function(UiEvent_NewChat) _then) = _$UiEvent_NewChatCopyWithImpl;
@useResult
$Res call({
 int index, String name, bool request
});




}
/// @nodoc
class _$UiEvent_NewChatCopyWithImpl<$Res>
    implements $UiEvent_NewChatCopyWith<$Res> {
  _$UiEvent_NewChatCopyWithImpl(this._self, this._then);

  final UiEvent_NewChat _self;
  final $Res Function(UiEvent_NewChat) _then;

/// Create a copy of UiEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? index = null,Object? name = null,Object? request = null,}) {
  return _then(UiEvent_NewChat(
index: null == index ? _self.index : index // ignore: cast_nullable_to_non_nullable
as int,name: null == name ? _self.name : name // ignore: cast_nullable_to_non_nullable
as String,request: null == request ? _self.request : request // ignore: cast_nullable_to_non_nullable
as bool,
  ));
}


}

/// @nodoc


class UiEvent_Avatar extends UiEvent {
  const UiEvent_Avatar({required this.handle}): super._();
  

 final  String handle;

/// Create a copy of UiEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiEvent_AvatarCopyWith<UiEvent_Avatar> get copyWith => _$UiEvent_AvatarCopyWithImpl<UiEvent_Avatar>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is UiEvent_Avatar&&(identical(other.handle, handle) || other.handle == handle));
}


@override
int get hashCode {
    return Object.hash(runtimeType,handle);
}

@override
String toString() {
    return 'UiEvent.avatar(handle: $handle)';
}


}

/// @nodoc
abstract mixin class $UiEvent_AvatarCopyWith<$Res> implements $UiEventCopyWith<$Res> {
  factory $UiEvent_AvatarCopyWith(UiEvent_Avatar value, $Res Function(UiEvent_Avatar) _then) = _$UiEvent_AvatarCopyWithImpl;
@useResult
$Res call({
 String handle
});




}
/// @nodoc
class _$UiEvent_AvatarCopyWithImpl<$Res>
    implements $UiEvent_AvatarCopyWith<$Res> {
  _$UiEvent_AvatarCopyWithImpl(this._self, this._then);

  final UiEvent_Avatar _self;
  final $Res Function(UiEvent_Avatar) _then;

/// Create a copy of UiEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? handle = null,}) {
  return _then(UiEvent_Avatar(
handle: null == handle ? _self.handle : handle // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class UiEvent_ChatsChanged extends UiEvent {
  const UiEvent_ChatsChanged(): super._();
  






@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is UiEvent_ChatsChanged);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'UiEvent.chatsChanged()';
}


}




/// @nodoc


class UiEvent_Message extends UiEvent {
  const UiEvent_Message({required this.index, required this.message, required this.local}): super._();
  

 final  int index;
 final  Message message;
 final  bool local;

/// Create a copy of UiEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiEvent_MessageCopyWith<UiEvent_Message> get copyWith => _$UiEvent_MessageCopyWithImpl<UiEvent_Message>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is UiEvent_Message&&(identical(other.index, index) || other.index == index)&&(identical(other.message, message) || other.message == message)&&(identical(other.local, local) || other.local == local));
}


@override
int get hashCode {
    return Object.hash(runtimeType,index,message,local);
}

@override
String toString() {
    return 'UiEvent.message(index: $index, message: $message, local: $local)';
}


}

/// @nodoc
abstract mixin class $UiEvent_MessageCopyWith<$Res> implements $UiEventCopyWith<$Res> {
  factory $UiEvent_MessageCopyWith(UiEvent_Message value, $Res Function(UiEvent_Message) _then) = _$UiEvent_MessageCopyWithImpl;
@useResult
$Res call({
 int index, Message message, bool local
});




}
/// @nodoc
class _$UiEvent_MessageCopyWithImpl<$Res>
    implements $UiEvent_MessageCopyWith<$Res> {
  _$UiEvent_MessageCopyWithImpl(this._self, this._then);

  final UiEvent_Message _self;
  final $Res Function(UiEvent_Message) _then;

/// Create a copy of UiEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? index = null,Object? message = null,Object? local = null,}) {
  return _then(UiEvent_Message(
index: null == index ? _self.index : index // ignore: cast_nullable_to_non_nullable
as int,message: null == message ? _self.message : message // ignore: cast_nullable_to_non_nullable
as Message,local: null == local ? _self.local : local // ignore: cast_nullable_to_non_nullable
as bool,
  ));
}


}

/// @nodoc


class UiEvent_Removed extends UiEvent {
  const UiEvent_Removed({required this.index}): super._();
  

 final  int index;

/// Create a copy of UiEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiEvent_RemovedCopyWith<UiEvent_Removed> get copyWith => _$UiEvent_RemovedCopyWithImpl<UiEvent_Removed>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is UiEvent_Removed&&(identical(other.index, index) || other.index == index));
}


@override
int get hashCode {
    return Object.hash(runtimeType,index);
}

@override
String toString() {
    return 'UiEvent.removed(index: $index)';
}


}

/// @nodoc
abstract mixin class $UiEvent_RemovedCopyWith<$Res> implements $UiEventCopyWith<$Res> {
  factory $UiEvent_RemovedCopyWith(UiEvent_Removed value, $Res Function(UiEvent_Removed) _then) = _$UiEvent_RemovedCopyWithImpl;
@useResult
$Res call({
 int index
});




}
/// @nodoc
class _$UiEvent_RemovedCopyWithImpl<$Res>
    implements $UiEvent_RemovedCopyWith<$Res> {
  _$UiEvent_RemovedCopyWithImpl(this._self, this._then);

  final UiEvent_Removed _self;
  final $Res Function(UiEvent_Removed) _then;

/// Create a copy of UiEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? index = null,}) {
  return _then(UiEvent_Removed(
index: null == index ? _self.index : index // ignore: cast_nullable_to_non_nullable
as int,
  ));
}


}

/// @nodoc


class UiEvent_UnverifiedMember extends UiEvent {
  const UiEvent_UnverifiedMember({required this.index, required this.name}): super._();
  

 final  int index;
 final  String name;

/// Create a copy of UiEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiEvent_UnverifiedMemberCopyWith<UiEvent_UnverifiedMember> get copyWith => _$UiEvent_UnverifiedMemberCopyWithImpl<UiEvent_UnverifiedMember>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is UiEvent_UnverifiedMember&&(identical(other.index, index) || other.index == index)&&(identical(other.name, name) || other.name == name));
}


@override
int get hashCode {
    return Object.hash(runtimeType,index,name);
}

@override
String toString() {
    return 'UiEvent.unverifiedMember(index: $index, name: $name)';
}


}

/// @nodoc
abstract mixin class $UiEvent_UnverifiedMemberCopyWith<$Res> implements $UiEventCopyWith<$Res> {
  factory $UiEvent_UnverifiedMemberCopyWith(UiEvent_UnverifiedMember value, $Res Function(UiEvent_UnverifiedMember) _then) = _$UiEvent_UnverifiedMemberCopyWithImpl;
@useResult
$Res call({
 int index, String name
});




}
/// @nodoc
class _$UiEvent_UnverifiedMemberCopyWithImpl<$Res>
    implements $UiEvent_UnverifiedMemberCopyWith<$Res> {
  _$UiEvent_UnverifiedMemberCopyWithImpl(this._self, this._then);

  final UiEvent_UnverifiedMember _self;
  final $Res Function(UiEvent_UnverifiedMember) _then;

/// Create a copy of UiEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? index = null,Object? name = null,}) {
  return _then(UiEvent_UnverifiedMember(
index: null == index ? _self.index : index // ignore: cast_nullable_to_non_nullable
as int,name: null == name ? _self.name : name // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class UiEvent_RelayReady extends UiEvent {
  const UiEvent_RelayReady({required this.addr}): super._();
  

 final  String addr;

/// Create a copy of UiEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiEvent_RelayReadyCopyWith<UiEvent_RelayReady> get copyWith => _$UiEvent_RelayReadyCopyWithImpl<UiEvent_RelayReady>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is UiEvent_RelayReady&&(identical(other.addr, addr) || other.addr == addr));
}


@override
int get hashCode {
    return Object.hash(runtimeType,addr);
}

@override
String toString() {
    return 'UiEvent.relayReady(addr: $addr)';
}


}

/// @nodoc
abstract mixin class $UiEvent_RelayReadyCopyWith<$Res> implements $UiEventCopyWith<$Res> {
  factory $UiEvent_RelayReadyCopyWith(UiEvent_RelayReady value, $Res Function(UiEvent_RelayReady) _then) = _$UiEvent_RelayReadyCopyWithImpl;
@useResult
$Res call({
 String addr
});




}
/// @nodoc
class _$UiEvent_RelayReadyCopyWithImpl<$Res>
    implements $UiEvent_RelayReadyCopyWith<$Res> {
  _$UiEvent_RelayReadyCopyWithImpl(this._self, this._then);

  final UiEvent_RelayReady _self;
  final $Res Function(UiEvent_RelayReady) _then;

/// Create a copy of UiEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? addr = null,}) {
  return _then(UiEvent_RelayReady(
addr: null == addr ? _self.addr : addr // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class UiEvent_FileProgress extends UiEvent {
  const UiEvent_FileProgress({required this.id, required this.done, required this.total}): super._();
  

 final  String id;
 final  int done;
 final  int total;

/// Create a copy of UiEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiEvent_FileProgressCopyWith<UiEvent_FileProgress> get copyWith => _$UiEvent_FileProgressCopyWithImpl<UiEvent_FileProgress>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is UiEvent_FileProgress&&(identical(other.id, id) || other.id == id)&&(identical(other.done, done) || other.done == done)&&(identical(other.total, total) || other.total == total));
}


@override
int get hashCode {
    return Object.hash(runtimeType,id,done,total);
}

@override
String toString() {
    return 'UiEvent.fileProgress(id: $id, done: $done, total: $total)';
}


}

/// @nodoc
abstract mixin class $UiEvent_FileProgressCopyWith<$Res> implements $UiEventCopyWith<$Res> {
  factory $UiEvent_FileProgressCopyWith(UiEvent_FileProgress value, $Res Function(UiEvent_FileProgress) _then) = _$UiEvent_FileProgressCopyWithImpl;
@useResult
$Res call({
 String id, int done, int total
});




}
/// @nodoc
class _$UiEvent_FileProgressCopyWithImpl<$Res>
    implements $UiEvent_FileProgressCopyWith<$Res> {
  _$UiEvent_FileProgressCopyWithImpl(this._self, this._then);

  final UiEvent_FileProgress _self;
  final $Res Function(UiEvent_FileProgress) _then;

/// Create a copy of UiEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? id = null,Object? done = null,Object? total = null,}) {
  return _then(UiEvent_FileProgress(
id: null == id ? _self.id : id // ignore: cast_nullable_to_non_nullable
as String,done: null == done ? _self.done : done // ignore: cast_nullable_to_non_nullable
as int,total: null == total ? _self.total : total // ignore: cast_nullable_to_non_nullable
as int,
  ));
}


}

// dart format on
