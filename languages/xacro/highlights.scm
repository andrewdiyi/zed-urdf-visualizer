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
  (#any-of? @keyword "robot" "link" "joint" "xacro:macro" "xacro:if" "xacro:unless"))
((ETag (Name) @keyword)
  (#any-of? @keyword "robot" "link" "joint" "xacro:macro" "xacro:if" "xacro:unless"))
((EmptyElemTag (Name) @keyword)
  (#any-of? @keyword "xacro:property" "xacro:arg" "xacro:include" "xacro:insert_block" "parent" "child" "origin" "axis" "limit" "mass" "inertia" "mesh"))

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
