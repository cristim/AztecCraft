pub fn specimen() -> Vec<u8> {
    br#"//Maya ASCII 2016 scene
requires maya "2016";
currentUnit -l centimeter -a degree -t film;
createNode transform -n "curve";
setAttr ".t" -type "double3" 12 0 0;
createNode nurbsCurve -n "curveShape" -p "curve";
setAttr ".cc" -type "nurbsCurve" 3 1 0 no 3 6 0 0 0 1 1 1 4 0 0 0 1 3 0 2 -3 0 3 0 0;
createNode script -n "uiConfigurationScriptNode";
setAttr ".b" -type "string" "error \"inert scene script\"; `unknown`; $payload;";
setAttr ".st" 3;
"#
    .to_vec()
}
