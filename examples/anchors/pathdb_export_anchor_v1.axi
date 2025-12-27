module PathDBExport

schema PathDBExportV1:
  object Entity
  object Relation
  object InternedStringId
  object Utf8String
  object Float32Bits
  relation interned_string(interned_id: InternedStringId, value: Utf8String)
  relation entity_type(entity: Entity, type_id: InternedStringId)
  relation entity_attribute(entity: Entity, key_id: InternedStringId, value_id: InternedStringId)
  relation relation_info(relation: Relation, rel_type_id: InternedStringId, source: Entity, target: Entity, confidence: Float32Bits)
  relation relation_attribute(relation: Relation, key_id: InternedStringId, value_id: InternedStringId)
  relation equivalence(entity: Entity, other: Entity, equiv_type_id: InternedStringId)

instance SnapshotV1 of PathDBExportV1:
  Entity = {Entity_0, Entity_1, Entity_2}
  Relation = {Relation_0, Relation_1}
  InternedStringId = {
    StringId_0,
    StringId_1,
    StringId_2,
    StringId_3,
    StringId_4,
    StringId_5,
    StringId_6
  }
  Utf8String = {
    StrUtf8Hex_5468696e67,
    StrUtf8Hex_6e616d65,
    StrUtf8Hex_61,
    StrUtf8Hex_62,
    StrUtf8Hex_63,
    StrUtf8Hex_7231,
    StrUtf8Hex_7232
  }
  Float32Bits = {F32Hex_3f4ccccd, F32Hex_3f666666}

  interned_string = {
    (interned_id=StringId_0, value=StrUtf8Hex_5468696e67),
    (interned_id=StringId_1, value=StrUtf8Hex_6e616d65),
    (interned_id=StringId_2, value=StrUtf8Hex_61),
    (interned_id=StringId_3, value=StrUtf8Hex_62),
    (interned_id=StringId_4, value=StrUtf8Hex_63),
    (interned_id=StringId_5, value=StrUtf8Hex_7231),
    (interned_id=StringId_6, value=StrUtf8Hex_7232)
  }
  entity_type = {
    (entity=Entity_0, type_id=StringId_0),
    (entity=Entity_1, type_id=StringId_0),
    (entity=Entity_2, type_id=StringId_0)
  }
  entity_attribute = {
    (entity=Entity_0, key_id=StringId_1, value_id=StringId_2),
    (entity=Entity_1, key_id=StringId_1, value_id=StringId_3),
    (entity=Entity_2, key_id=StringId_1, value_id=StringId_4)
  }
  relation_info = {
    (relation=Relation_0, rel_type_id=StringId_5, source=Entity_0, target=Entity_1, confidence=F32Hex_3f666666),
    (relation=Relation_1, rel_type_id=StringId_6, source=Entity_1, target=Entity_2, confidence=F32Hex_3f4ccccd)
  }
  relation_attribute = {}
  equivalence = {}
