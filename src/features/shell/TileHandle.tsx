import { createContext, useContext, type ReactNode } from "react";

/** The drag handle of the tile being rendered, if it sits in a movable layout. Tiles show it at the end of their header. */
export const TileHandleContext = createContext<ReactNode>(null);
export const useTileHandle = () => useContext(TileHandleContext);
