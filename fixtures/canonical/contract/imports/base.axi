module Base
schema Shared
  object Person
  relation Parent(child: Person, parent: Person)
