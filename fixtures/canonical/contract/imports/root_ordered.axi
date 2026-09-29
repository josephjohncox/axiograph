module Root
import Other
import Base
instance Family of Shared:
  Person = {Alice, Bob}
  Parent = {(child=Alice, parent=Bob)}
