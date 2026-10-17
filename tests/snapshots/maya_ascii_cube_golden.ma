//Maya ASCII 2026 scene
//Original text written for AztecCraft. Its mesh arrays follow the structure of the Autodesk
//maya-usd UsdExportMeshTest.ma fixture (Apache-2.0); no bytes are copied from it.
requires maya "2026";
currentUnit -l centimeter -a degree -t film;
fileInfo "AztecCraft" "1";
createNode transform -n "cube";
	rename -uid "6C1B2D4E-0A3F-4B5C-8D7E-9F0A1B2C3D4E";
	setAttr ".t" -type "double3" 1.5 -0 0.1;
	setAttr ".s" -type "double3" 1 1 1;
createNode mesh -n "cubeShape" -p "cube";
	rename -uid "7D2C3E5F-1B40-4C6D-9E8F-A01B2C3D4E5F";
	setAttr -s 8 ".vt[0:7]"  -1 -2 3 1 -2 3 -1 2 3 1 2 3 -1 2 -3 1 2 -3 -1 -2 -3 1 -2 -3;
	setAttr -s 12 ".ed[0:11]"  0 1 0 2 3 0 4 5 0 6 7 0 0 2 0 1 3 0 2 4 0 3 5 0 4 6 0 5 7 0
		6 0 0 7 1 0;
	setAttr -s 6 ".fc[0:5]" -type "polyFaces"
		f 4 0 5 -2 -5
		f 4 1 7 -3 -7
		f 4 2 9 -4 -9
		f 4 3 11 -1 -11
		f 4 -12 -10 -8 -6
		f 4 10 4 6 8;
createNode lambert -n "red";
	setAttr ".c" -type "float3" 0.1 0.3333333333333333 0.9;
createNode shadingEngine -n "redSG";
connectAttr "red.oc" "redSG.ss";
connectAttr "cubeShape.iog" "redSG.dsm" -na;
connectAttr "cubeShape.iog" ":initialShadingGroup.dsm" -na;
