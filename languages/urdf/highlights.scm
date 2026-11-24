"xml" @keyword
[ "version" "encoding" "standalone" ] @property
(EncName) @string.special
(VersionNum) @number

(PI) @embedded
(PI (PITarget) @keyword)

(EntityRef) @constant
(CharRef) @constant

(STag (Name) @tag)
(ETag (Name) @tag)
(EmptyElemTag (Name) @tag)

((STag (Name) @keyword)
  (#any-of? @keyword "robot" "link" "joint" "transmission" "gazebo"))
((ETag (Name) @keyword)
  (#any-of? @keyword "robot" "link" "joint" "transmission" "gazebo"))
((EmptyElemTag (Name) @keyword)
  (#any-of? @keyword "parent" "child" "origin" "axis" "limit" "mass" "inertia" "mesh" "box" "cylinder" "sphere"))

(Attribute (Name) @property)
(Attribute (AttValue) @string)

[
  "<?" "?>"
  "<!" "]]>"
  "<" ">"
  "</" "/>"
] @punctuation.delimiter

[ "\"" "'" ] @punctuation.delimiter
"=" @operator

(CharData) @markup
(Comment) @comment
(ERROR) @error
