// Preparation and fragment traversal have different register/branch costs.
// Keep the fragment's original early parallel rejection; the ambient compute
// program specializes this constant to defer edge lengths until a candidate hit.
const POSED_DEFER_PARALLEL_TEST:bool = false;
