// Minimal asynchronous storage double. Real IndexedDB persistence is tested in Chromium.
export function outboxStorage() {
 const items=new Map();
 const db={close(){},createObjectStore(){},transaction(){
  const tx={objectStore(){return {
   getAll(){const request={};queueMicrotask(()=>{request.result=[...items.values()];request.onsuccess?.();});return request;},
   put(item){items.set(item.key,structuredClone(item));queueMicrotask(()=>tx.oncomplete?.());},
   delete(key){items.delete(key);queueMicrotask(()=>tx.oncomplete?.());}
  };}};
  return tx;
 }};
 return {open(){const request={};queueMicrotask(()=>{request.result=db;request.onsuccess?.();});return request;}};
}
