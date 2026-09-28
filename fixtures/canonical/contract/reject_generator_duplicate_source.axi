module GeneratorTupleParity
schema S
  object A
  object B
  function F: A -> B
instance I of S
  F = {(source=a, source=b, target=c)}
