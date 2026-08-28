module Demo
schema S:
  object Person
  relation Parent(child: Person, parent: Person)
instance I of S:
  Person = {Alice, Bob}
  Parent = {(child=Alice, parent=Bob)}
