module RepeatedSubtype

schema S:
  object Child
  object Parent
  subtype Child < Parent
  subtype Child < Parent
