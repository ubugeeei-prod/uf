export type Omit<TObject, TKey> = TObject;
export type OmitKeyof<TObject, TKey extends keyof any> = Omit<TObject, TKey>;
