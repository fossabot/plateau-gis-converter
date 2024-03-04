const filetypeOptions: Record<string, { label: string; extensions: string[] }> = {
	geojson: {
		label: 'GeoJSON',
		extensions: []
	},
	gpkg: {
		label: 'GeoPackage',
		extensions: ['gpkg']
	},
	mvt: {
		label: 'Vector Tiles',
		extensions: ['']
	},
	czml: {
		label: 'CZML',
		extensions: ['json']
	},
	cesiumtiles: {
		label: '3D Tiles',
		extensions: ['']
	},
	kml: {
		label: 'KML',
		extensions: ['kml']
	},
	shapefile: {
		label: 'Shapefile',
		extensions: ['shp']
	},
	ply: {
		label: 'PLY',
		extensions: ['ply']
	},
	gltf: {
		label: 'glTF',
		extensions: ['']
	},
	serde: {
		label: 'Serde',
		extensions: ['']
	}
};

const crsOptions = [
	{ value: 'EPSG:6678', label: 'JGD2011 / Japan Plane Rectangular CS X' },
	{ value: 'EPSG:4326', label: 'WGS 84' },
	{ value: 'EPSG:3857', label: 'Web Mercator' }
];

export { crsOptions, filetypeOptions };
